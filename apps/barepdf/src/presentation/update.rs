use crate::presentation::commands::AppCommand;
use crate::presentation::message::Msg;
use crate::presentation::model::AppModel;
use barepdf_core::SecretPassword;

/// Pure deterministic state transition function in Model-View-Update (TEA).
pub fn update(model: &mut AppModel, msg: Msg) -> Option<AppCommand> {
    match msg {
        Msg::NextPage => {
            if let Some(tab) = model.active_tab_mut() {
                if let Some(next) = tab.current_page.next(tab.page_count) {
                    tab.current_page = next;
                    return Some(AppCommand::RequestPageRender {
                        document_id: tab.id,
                        page_index: next,
                    });
                }
            }
            None
        }
        Msg::PreviousPage => {
            if let Some(tab) = model.active_tab_mut() {
                if let Some(prev) = tab.current_page.prev() {
                    tab.current_page = prev;
                    return Some(AppCommand::RequestPageRender {
                        document_id: tab.id,
                        page_index: prev,
                    });
                }
            }
            None
        }
        Msg::GoToPage(page_index) => {
            if let Some(tab) = model.active_tab_mut() {
                if page_index.get() < tab.page_count {
                    tab.current_page = page_index;
                    return Some(AppCommand::RequestPageRender {
                        document_id: tab.id,
                        page_index,
                    });
                }
            }
            None
        }
        Msg::ToggleZenMode => {
            model.zen_mode = !model.zen_mode;
            if model.zen_mode {
                model.sidebar_open = false;
            }
            Some(AppCommand::SyncWindowChrome)
        }
        Msg::ToggleCommandPalette => {
            model.command_palette_open = !model.command_palette_open;
            if model.command_palette_open {
                model.command_palette_query.clear();
            }
            None
        }
        Msg::CommandPaletteQueryChanged(query) => {
            model.command_palette_query = query;
            None
        }
        Msg::ExecuteCommand(query) => {
            crate::presentation::hud_commands::handle_hud_query(model, &query)
        }
        Msg::SetPaperTint(tint) => {
            model.paper_tint = tint;
            model.preferences.paper_tint = tint.as_u8();
            Some(AppCommand::InvalidateCanvas)
        }
        Msg::SetViewingMode(mode) => {
            model.viewing_mode = mode;
            model.preferences.viewing_mode = mode;
            Some(AppCommand::InvalidateCanvas)
        }
        Msg::SetZoom(mode) => {
            model.zoom_mode = mode;
            model.preferences.zoom_mode = mode;
            Some(AppCommand::InvalidateCanvas)
        }
        Msg::ZoomIn => {
            // Zoom in effect
            Some(AppCommand::InvalidateCanvas)
        }
        Msg::ZoomOut => {
            // Zoom out effect
            Some(AppCommand::InvalidateCanvas)
        }
        Msg::RotateClockwise => Some(AppCommand::InvalidateCanvas),
        Msg::ScrollPositionChanged(scroll_y) => {
            if let Some(tab) = model.active_tab_mut() {
                tab.scroll_y = scroll_y;
            }
            None
        }
        Msg::CloseTab(index) => {
            if index < model.tabs.len() {
                model.tabs.remove(index);
                if model.active_tab_index >= model.tabs.len() && !model.tabs.is_empty() {
                    model.active_tab_index = model.tabs.len() - 1;
                }
                Some(AppCommand::SavePreferences)
            } else {
                None
            }
        }
        Msg::SelectTab(index) => {
            if index < model.tabs.len() {
                model.active_tab_index = index;
                model.preferences.active_tab_index = index;
                Some(AppCommand::InvalidateCanvas)
            } else {
                None
            }
        }
        Msg::OpenDocument(path) => Some(AppCommand::OpenDocumentPath {
            path,
            password: None,
        }),
        Msg::UnlockPasswordSubmitted(password) => {
            model.active_tab().map(|tab| AppCommand::OpenDocumentPath {
                path: tab.path.clone(),
                password: Some(SecretPassword::new(password)),
            })
        }
        Msg::UnlockPasswordCancelled => None,
        Msg::WindowResized { width, height } => {
            model.preferences.last_window_width = width;
            model.preferences.last_window_height = height;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::model::{DocumentTabModel, PaperTintColor};
    use barepdf_core::{DocumentId, PageIndex};
    use std::path::PathBuf;

    fn make_test_model() -> AppModel {
        let mut model = AppModel::default();
        model.tabs.push(DocumentTabModel {
            id: DocumentId::new(1),
            path: PathBuf::from("test.pdf"),
            file_name: "test.pdf".to_string(),
            page_count: 5,
            current_page: PageIndex::from_raw(0),
            scroll_y: 0.0,
        });
        model
    }

    #[test]
    fn next_page_advances_current_page_and_requests_render() {
        let mut model = make_test_model();
        let cmd = update(&mut model, Msg::NextPage);
        assert_eq!(
            cmd,
            Some(AppCommand::RequestPageRender {
                document_id: DocumentId::new(1),
                page_index: PageIndex::from_raw(1),
            })
        );
        assert_eq!(model.tabs[0].current_page.get(), 1);
    }

    #[test]
    fn previous_page_at_first_page_does_nothing() {
        let mut model = make_test_model();
        let cmd = update(&mut model, Msg::PreviousPage);
        assert_eq!(cmd, None);
        assert_eq!(model.tabs[0].current_page.get(), 0);
    }

    #[test]
    fn zen_mode_toggles_and_closes_sidebar() {
        let mut model = make_test_model();
        model.sidebar_open = true;
        let cmd = update(&mut model, Msg::ToggleZenMode);
        assert!(model.zen_mode);
        assert!(!model.sidebar_open);
        assert_eq!(cmd, Some(AppCommand::SyncWindowChrome));
    }

    #[test]
    fn paper_tint_updates_model_and_invalidates_canvas() {
        let mut model = make_test_model();
        let cmd = update(&mut model, Msg::SetPaperTint(PaperTintColor::WarmSepia));
        assert_eq!(model.paper_tint, PaperTintColor::WarmSepia);
        assert_eq!(model.preferences.paper_tint, 1);
        assert_eq!(cmd, Some(AppCommand::InvalidateCanvas));
    }
}
