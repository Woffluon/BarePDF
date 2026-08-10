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
    m.insert("settings.view_mode", "Default View Mode");
    m.insert("settings.reading_dir", "Reading Direction");
    m.insert("settings.reading_dir.ltr", "Left to Right (LTR)");
    m.insert("settings.reading_dir.rtl", "Right to Left (RTL)");
    m.insert("settings.close", "Close");
    m.insert("password.title", "Password Required");
    m.insert(
        "password.desc",
        "This document is encrypted. Enter password to open:",
    );
    m.insert("password.placeholder", "Password");
    m.insert("password.unlock", "Unlock");
    m.insert("password.cancel", "Cancel");
    m.insert("context.copy", "Copy");
    m.insert("context.select_all", "Select All");
    m.insert("status.ready", "Ready");
    m.insert("status.opening", "Opening document...");
    m.insert("status.opened", "Opened {name} ({pages} pages)");
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
    m.insert("settings.view_mode", "Varsayılan Görünüm Mode");
    m.insert("settings.reading_dir", "Okuma Yönü");
    m.insert("settings.reading_dir.ltr", "Soldan Sağa (LTR)");
    m.insert("settings.reading_dir.rtl", "Sağdan Sola (RTL)");
    m.insert("settings.close", "Kapat");
    m.insert("password.title", "Parola Gerekli");
    m.insert(
        "password.desc",
        "Bu belge şifrelenmiş. Açmak için parolayı girin:",
    );
    m.insert("password.placeholder", "Parola");
    m.insert("password.unlock", "Kilidi Aç");
    m.insert("password.cancel", "İptal");
    m.insert("context.copy", "Kopyala");
    m.insert("context.select_all", "Tümünü Seç");
    m.insert("status.ready", "Hazır");
    m.insert("status.opening", "Belge açılıyor...");
    m.insert("status.opened", "{name} açıldı ({pages} sayfa)");
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
        use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
        let lang_id = unsafe { GetUserDefaultUILanguage() };
        let primary_lang = lang_id & 0x3FF; // LANGID primary language bits
        if primary_lang == 0x1F {
            // LANG_TURKISH
            return ResolvedLanguage::Turkish;
        }
    }
    ResolvedLanguage::English
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
        assert_eq!(t(ResolvedLanguage::Turkish, "nonexistent"), "");
    }
}
