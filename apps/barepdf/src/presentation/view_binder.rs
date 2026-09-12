use crate::presentation::commands::AppCommand;
use crate::presentation::hud_commands::filter_hud_commands;
use crate::presentation::model::AppModel;
use crate::presentation::state::AppState;
use crate::presentation::ui::{
    begin_open, invalidate_layout_and_render, navigate_to_page, persist_preferences, show_banner,
    zoom_mode_index,
};
use barepdf_render::RenderScheduler;
use barepdf_ui::AppWindow;
use slint::{ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Synchronizes state from AppModel to the Slint AppWindow instance.
pub(super) fn sync_model_to_window(model: &AppModel, window: &AppWindow) {
    window.set_paper_tint(i32::from(model.paper_tint.as_u8()));
    window.set_zen_mode(model.zen_mode);
    window.set_sidebar_visible(!model.zen_mode && model.sidebar_open);
    sync_hud_palette(model, window);
}

/// Synchronizes HUD command palette search results and visibility with the Slint window.
pub(super) fn sync_hud_palette(model: &AppModel, window: &AppWindow) {
    window.set_command_palette_open(model.command_palette_open);
    window.set_command_palette_query(SharedString::from(&model.command_palette_query));

    let matching = filter_hud_commands(&model.command_palette_query);
    let titles: Vec<SharedString> = matching
        .iter()
        .map(|c| SharedString::from(c.title))
        .collect();
    let subtitles: Vec<SharedString> = matching
        .iter()
        .map(|c| SharedString::from(c.subtitle))
        .collect();

    window.set_command_palette_titles(ModelRc::new(VecModel::from(titles)));
    window.set_command_palette_subtitles(ModelRc::new(VecModel::from(subtitles)));
}

/// Executes an `AppCommand` effect produced by pure TEA state transitions.
pub(super) fn execute_command_effect(
    cmd: AppCommand,
    model: &AppModel,
    state: &Rc<RefCell<AppState>>,
    scheduler: &RenderScheduler,
    window: &AppWindow,
    preferences_path: &Path,
) {
    match cmd {
        AppCommand::RequestPageRender { page_index, .. } => {
            navigate_to_page(page_index.get(), state, scheduler, window);
        }
        AppCommand::SyncWindowChrome => {
            window.set_zen_mode(model.zen_mode);
            window.set_sidebar_visible(!model.zen_mode && model.sidebar_open);
        }
        AppCommand::InvalidateCanvas => {
            let mut app = state.borrow_mut();
            app.zoom_mode = model.zoom_mode;
            window.set_zoom_mode(zoom_mode_index(model.zoom_mode));
            app.preferences.paper_tint = model.paper_tint.as_u8();
            persist_preferences(&app.preferences, preferences_path, Some(window));
            invalidate_layout_and_render(&mut app, scheduler, window, false);
        }
        AppCommand::ExecutePrintDialog => {
            window.invoke_request_print();
        }
        AppCommand::OpenDocumentPath { path, password } => {
            begin_open(path, password, state, scheduler, window);
        }
        AppCommand::SavePreferences => {
            let app = state.borrow();
            persist_preferences(&app.preferences, preferences_path, Some(window));
        }
        AppCommand::ShowBanner { message, can_retry } => {
            show_banner(window, &message, can_retry);
        }
    }
}
