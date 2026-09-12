use crate::presentation::commands::AppCommand;
use crate::presentation::model::{AppModel, PaperTintColor};
use barepdf_core::PageIndex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HudCommandItem {
    pub id: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
}

pub const ALL_HUD_COMMANDS: &[HudCommandItem] = &[
    HudCommandItem {
        id: "zen",
        title: "Toggle Zen Reading Mode (F11)",
        subtitle: "Focus view without distractions",
    },
    HudCommandItem {
        id: "tint_sepia",
        title: "Paper Tint: Warm Sepia",
        subtitle: "Eye comfort mode for book reading",
    },
    HudCommandItem {
        id: "tint_night",
        title: "Paper Tint: Dark Invert",
        subtitle: "Inverted high contrast night reading",
    },
    HudCommandItem {
        id: "tint_amber",
        title: "Paper Tint: OLED Amber",
        subtitle: "Warm amber glow on deep black",
    },
    HudCommandItem {
        id: "tint_normal",
        title: "Paper Tint: Original",
        subtitle: "Standard PDF colors",
    },
    HudCommandItem {
        id: "print",
        title: "Print Document (Ctrl+P)",
        subtitle: "Open native Windows print preview",
    },
    HudCommandItem {
        id: "fit_width",
        title: "Zoom: Fit to Width",
        subtitle: "Scale document page to window width",
    },
    HudCommandItem {
        id: "fit_page",
        title: "Zoom: Fit Entire Page",
        subtitle: "Scale document page to fit window",
    },
];

/// Returns matching command suggestions for the given query.
#[must_use]
pub fn filter_hud_commands(query: &str) -> Vec<HudCommandItem> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return ALL_HUD_COMMANDS.to_vec();
    }
    ALL_HUD_COMMANDS
        .iter()
        .filter(|cmd| {
            cmd.title.to_lowercase().contains(&q)
                || cmd.subtitle.to_lowercase().contains(&q)
                || cmd.id.contains(&q)
        })
        .cloned()
        .collect()
}

/// Dispatches a selected or typed HUD command string.
pub fn handle_hud_query(model: &mut AppModel, query: &str) -> Option<AppCommand> {
    let trimmed = query.trim();

    // Check if query is a page number (e.g. "42")
    if let Ok(page_num) = trimmed.parse::<u32>() {
        if page_num >= 1 {
            let target_index = PageIndex::from_raw(page_num - 1);
            let target_cmd = if let Some(tab) = model.active_tab_mut() {
                if target_index.get() < tab.page_count {
                    tab.current_page = target_index;
                    Some(AppCommand::RequestPageRender {
                        document_id: tab.id,
                        page_index: target_index,
                    })
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(cmd) = target_cmd {
                model.command_palette_open = false;
                return Some(cmd);
            }
        }
    }

    let lower = trimmed.to_lowercase();
    model.command_palette_open = false;

    if lower.contains("zen") || lower == "f11" {
        model.zen_mode = !model.zen_mode;
        if model.zen_mode {
            model.sidebar_open = false;
        }
        Some(AppCommand::SyncWindowChrome)
    } else if lower.contains("sepia") || lower.contains("sepya") {
        model.paper_tint = PaperTintColor::WarmSepia;
        model.preferences.paper_tint = 1;
        Some(AppCommand::InvalidateCanvas)
    } else if lower.contains("night") || lower.contains("gece") || lower.contains("dark") {
        model.paper_tint = PaperTintColor::Night;
        model.preferences.paper_tint = 2;
        Some(AppCommand::InvalidateCanvas)
    } else if lower.contains("amber") || lower.contains("kehribar") {
        model.paper_tint = PaperTintColor::OledAmber;
        model.preferences.paper_tint = 3;
        Some(AppCommand::InvalidateCanvas)
    } else if lower.contains("normal") || lower.contains("orijinal") || lower.contains("original") {
        model.paper_tint = PaperTintColor::Normal;
        model.preferences.paper_tint = 0;
        Some(AppCommand::InvalidateCanvas)
    } else if lower.contains("print") || lower.contains("yazdır") {
        Some(AppCommand::ExecutePrintDialog)
    } else if lower.contains("fit width") || lower.contains("genişlik") {
        model.zoom_mode = barepdf_core::ZoomMode::FitWidth;
        model.preferences.zoom_mode = barepdf_core::ZoomMode::FitWidth;
        Some(AppCommand::InvalidateCanvas)
    } else if lower.contains("fit page") || lower.contains("sayfa sığdır") {
        model.zoom_mode = barepdf_core::ZoomMode::FitPage;
        model.preferences.zoom_mode = barepdf_core::ZoomMode::FitPage;
        Some(AppCommand::InvalidateCanvas)
    } else if !trimmed.is_empty() {
        Some(AppCommand::ShowBanner {
            message: format!("Unknown command: {trimmed}"),
            can_retry: false,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_hud_commands_empty_query_returns_all() {
        let commands = filter_hud_commands("");
        assert_eq!(commands.len(), ALL_HUD_COMMANDS.len());
    }

    #[test]
    fn filter_hud_commands_matches_substring() {
        let commands = filter_hud_commands("sepia");
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].id, "tint_sepia");
    }

    #[test]
    fn handle_hud_query_dispatches_page_navigation() {
        let mut model = AppModel::default();
        model
            .tabs
            .push(crate::presentation::model::DocumentTabModel {
                id: barepdf_core::DocumentId::new(10),
                path: std::path::PathBuf::from("doc.pdf"),
                file_name: "doc.pdf".to_string(),
                page_count: 50,
                current_page: PageIndex::from_raw(0),
                scroll_y: 0.0,
            });

        let cmd = handle_hud_query(&mut model, "42");
        assert_eq!(
            cmd,
            Some(AppCommand::RequestPageRender {
                document_id: barepdf_core::DocumentId::new(10),
                page_index: PageIndex::from_raw(41),
            })
        );
        assert_eq!(model.tabs[0].current_page.get(), 41);
    }
}
