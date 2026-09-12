use barepdf_core::{DocumentId, PageIndex, UserPreferences, ViewingMode, ZoomMode};
use std::path::PathBuf;

/// Paper tint color mode for reading ergonomics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaperTintColor {
    #[default]
    Normal = 0,
    WarmSepia = 1,
    Night = 2,
    OledAmber = 3,
}

impl PaperTintColor {
    #[must_use]
    pub const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::WarmSepia,
            2 => Self::Night,
            3 => Self::OledAmber,
            _ => Self::Normal,
        }
    }

    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    #[allow(dead_code)]
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Normal => "Original",
            Self::WarmSepia => "Warm Sepia",
            Self::Night => "Dark Invert",
            Self::OledAmber => "OLED Amber",
        }
    }
}

/// Active sidebar tab view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarTab {
    #[default]
    Thumbnails = 0,
    Outline = 1,
}

/// Presentation model representing a single document tab.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentTabModel {
    pub id: DocumentId,
    pub path: PathBuf,
    pub file_name: String,
    pub page_count: u32,
    pub current_page: PageIndex,
    pub scroll_y: f32,
}

/// Pure presentation state for BarePDF following Model-View-Update (TEA).
#[derive(Debug, Clone)]
pub struct AppModel {
    pub active_tab_index: usize,
    pub tabs: Vec<DocumentTabModel>,
    pub viewing_mode: ViewingMode,
    pub zoom_mode: ZoomMode,
    pub sidebar_open: bool,
    #[allow(dead_code)]
    pub sidebar_tab: SidebarTab,
    pub command_palette_open: bool,
    pub command_palette_query: String,
    pub zen_mode: bool,
    pub paper_tint: PaperTintColor,
    pub preferences: UserPreferences,
}

impl Default for AppModel {
    fn default() -> Self {
        let preferences = UserPreferences::default();
        Self {
            active_tab_index: 0,
            tabs: Vec::new(),
            viewing_mode: preferences.viewing_mode,
            zoom_mode: preferences.zoom_mode,
            sidebar_open: preferences.sidebar_visible,
            sidebar_tab: SidebarTab::Thumbnails,
            command_palette_open: false,
            command_palette_query: String::new(),
            zen_mode: false,
            paper_tint: PaperTintColor::from_u8(preferences.paper_tint),
            preferences,
        }
    }
}

impl AppModel {
    #[must_use]
    pub fn active_tab(&self) -> Option<&DocumentTabModel> {
        self.tabs.get(self.active_tab_index)
    }

    #[must_use]
    pub fn active_tab_mut(&mut self) -> Option<&mut DocumentTabModel> {
        self.tabs.get_mut(self.active_tab_index)
    }

    pub(super) fn from_app_state(
        app: &crate::presentation::state::AppState,
        window: &barepdf_ui::AppWindow,
    ) -> Self {
        let active_id = app.application.tabs.active_id();
        let mut active_tab_index = 0;
        let mut tabs = Vec::new();
        for (i, tab) in app.application.tabs.tabs().iter().enumerate() {
            if Some(tab.id) == active_id {
                active_tab_index = i;
            }
            if let Some(path) = &tab.path {
                tabs.push(DocumentTabModel {
                    id: DocumentId::new(tab.id.get()),
                    path: path.clone(),
                    file_name: tab.title.clone(),
                    page_count: app.page_count(),
                    current_page: tab.view.current_page,
                    scroll_y: tab.view.scroll_y,
                });
            }
        }
        Self {
            active_tab_index,
            tabs,
            viewing_mode: app.viewing_mode,
            zoom_mode: app.zoom_mode,
            sidebar_open: window.get_sidebar_visible(),
            sidebar_tab: if window.get_sidebar_tab() == 1 {
                SidebarTab::Outline
            } else {
                SidebarTab::Thumbnails
            },
            command_palette_open: window.get_command_palette_open(),
            command_palette_query: window.get_command_palette_query().to_string(),
            zen_mode: window.get_zen_mode(),
            paper_tint: PaperTintColor::from_u8(app.preferences.paper_tint),
            preferences: app.preferences.clone(),
        }
    }
}
