use crate::presentation::state::AppState;

use crate::presentation::ui::{invalidate_layout_and_render, zoom_mode_index};
use barepdf_render::RenderScheduler;
use barepdf_ui::AppWindow;
use slint::ComponentHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HudCommandItem {
    pub id: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
}

pub const ALL_HUD_COMMANDS: &[HudCommandItem] = &[
    HudCommandItem {
        id: "zen",
        title: "Toggle Full Screen (F11)",
        subtitle: "Focus view without distractions",
    },
    HudCommandItem {
        id: "invert_colors",
        title: "Toggle Inverted Page Colors (Ctrl+I)",
        subtitle: "High contrast inverted reading mode",
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
    HudCommandItem {
        id: "view_book",
        title: "View Mode: Book Mode",
        subtitle: "Two pages with first page as cover",
    },
    HudCommandItem {
        id: "view_continuous",
        title: "View Mode: Continuous",
        subtitle: "Scroll pages vertically",
    },
    HudCommandItem {
        id: "view_single",
        title: "View Mode: Single Page",
        subtitle: "Display one page at a time",
    },
    HudCommandItem {
        id: "view_two_page",
        title: "View Mode: Two Pages",
        subtitle: "Side-by-side spread view",
    },
];

fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

#[must_use]
pub fn filter_hud_commands(query: &str) -> Vec<HudCommandItem> {
    let q = query.trim();
    if q.is_empty() {
        return ALL_HUD_COMMANDS.to_vec();
    }

    ALL_HUD_COMMANDS
        .iter()
        .filter(|cmd| {
            contains_ignore_ascii_case(cmd.title, q)
                || contains_ignore_ascii_case(cmd.subtitle, q)
                || contains_ignore_ascii_case(cmd.id, q)
        })
        .cloned()
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HudAction {
    None,
    NavigateToPage(u32),
    ToggleFullscreen,
    ToggleInvertColors,
    SetPaperTint(u8),
    RequestPrint,
    SetZoomMode(barepdf_core::ZoomMode),
    SetViewingMode(barepdf_core::ViewingMode),
}

#[must_use]
pub fn parse_hud_action(query: &str) -> HudAction {
    let trimmed = query.trim();
    if let Ok(page_num) = trimmed.parse::<u32>() {
        if page_num >= 1 {
            return HudAction::NavigateToPage(page_num - 1);
        }
    }

    let lower = trimmed.to_lowercase();
    if lower == "zen" || lower.contains("zen") || lower.contains("fullscreen") || lower == "f11" {
        HudAction::ToggleFullscreen
    } else if lower == "invert_colors" || lower.contains("invert") || lower.contains("ters") {
        HudAction::ToggleInvertColors
    } else if lower == "tint_sepia" || lower.contains("sepia") || lower.contains("sepya") {
        HudAction::SetPaperTint(1)
    } else if lower == "tint_night"
        || lower.contains("night")
        || lower.contains("gece")
        || lower.contains("dark")
    {
        HudAction::SetPaperTint(2)
    } else if lower == "tint_amber" || lower.contains("amber") || lower.contains("kehribar") {
        HudAction::SetPaperTint(3)
    } else if lower == "tint_normal"
        || lower.contains("normal")
        || lower.contains("orijinal")
        || lower.contains("original")
    {
        HudAction::SetPaperTint(0)
    } else if lower.contains("print") || lower.contains("yazd") {
        HudAction::RequestPrint
    } else if lower == "fit_width" || lower.contains("fit width") || lower.contains("geni") {
        HudAction::SetZoomMode(barepdf_core::ZoomMode::FitWidth)
    } else if lower == "fit_page" || lower.contains("fit page") || lower.contains("sayfa s") {
        HudAction::SetZoomMode(barepdf_core::ZoomMode::FitPage)
    } else if lower == "view_book" || lower.contains("book") || lower.contains("kitap") {
        HudAction::SetViewingMode(barepdf_core::ViewingMode::BookMode)
    } else if lower == "view_two_page"
        || lower.contains("two page")
        || lower.contains("çift")
        || lower.contains("spread")
    {
        HudAction::SetViewingMode(barepdf_core::ViewingMode::TwoPageSpread)
    } else if lower == "view_single" || lower.contains("single page") || lower.contains("tek sayfa")
    {
        HudAction::SetViewingMode(barepdf_core::ViewingMode::SinglePage)
    } else if lower == "view_continuous"
        || lower.contains("continuous")
        || lower.contains("sürekli")
    {
        HudAction::SetViewingMode(barepdf_core::ViewingMode::ContinuousVertical)
    } else {
        HudAction::None
    }
}

pub fn execute_hud_action(
    action: HudAction,
    app: &mut AppState,
    scheduler: &RenderScheduler,
    window: &AppWindow,
) {
    match action {
        HudAction::None => {}
        HudAction::NavigateToPage(target_index) => {
            super::ui::navigate_to_page_inner(target_index, app, scheduler, window);
        }
        HudAction::ToggleFullscreen => {
            let new_mode = app.request_toggle_fullscreen();
            let enabled = new_mode == barepdf_core::WindowMode::FullScreen;
            window.set_window_mode(if enabled { 1 } else { 0 });
            window.window().set_fullscreen(enabled);
            super::window_chrome::sync_window_maximized(window);
        }
        HudAction::ToggleInvertColors => {
            app.preferences.invert_colors = !app.preferences.invert_colors;
            scheduler.set_invert_colors(app.preferences.invert_colors);
            window.set_invert_page_colors(app.preferences.invert_colors);
            invalidate_layout_and_render(app, scheduler, window, true);
        }
        HudAction::SetPaperTint(tint) => {
            app.preferences.paper_tint = tint;
            window.set_paper_tint(i32::from(tint));
            invalidate_layout_and_render(app, scheduler, window, false);
        }
        HudAction::RequestPrint => {}
        HudAction::SetZoomMode(mode) => {
            app.zoom_mode = mode;
            app.preferences.zoom_mode = mode;
            window.set_zoom_mode(zoom_mode_index(app.zoom_mode));
            invalidate_layout_and_render(app, scheduler, window, false);
        }
        HudAction::SetViewingMode(mode) => {
            app.viewing_mode = mode;
            app.preferences.viewing_mode = mode;
            app.layout_key = None;
            window.set_view_mode(crate::presentation::ui::view_mode_index(app.viewing_mode));
            window.set_view_mode_label(slint::SharedString::from(
                crate::presentation::ui::view_mode_label(
                    app.viewing_mode,
                    app.preferences.language.resolve(),
                ),
            ));
            crate::presentation::ui::navigate_to_page_inner(
                app.current_page,
                app,
                scheduler,
                window,
            );
        }
    }
}

pub fn execute_hud_command(
    app: &mut AppState,
    scheduler: &RenderScheduler,
    window: &AppWindow,
    query: &str,
) -> HudAction {
    handle_hud_query(app, scheduler, window, query)
}

pub fn handle_hud_query(
    app: &mut AppState,
    scheduler: &RenderScheduler,
    window: &AppWindow,
    query: &str,
) -> HudAction {
    let action = parse_hud_action(query);
    window.set_command_palette_open(false);

    if action == HudAction::None {
        let trimmed = query.trim();
        if !trimmed.is_empty() {
            crate::presentation::ui::show_banner(
                window,
                format!("Unknown command: {trimmed}"),
                false,
            );
        }
    } else {
        execute_hud_action(action, app, scheduler, window);
    }

    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn parse_hud_action_identifies_all_commands() {
        assert_eq!(parse_hud_action("print"), HudAction::RequestPrint);
        assert_eq!(parse_hud_action("PRINT"), HudAction::RequestPrint);
        assert_eq!(parse_hud_action("  print  "), HudAction::RequestPrint);
        assert_eq!(parse_hud_action("yazdır"), HudAction::RequestPrint);
        assert_eq!(
            parse_hud_action("Print Document (Ctrl+P)"),
            HudAction::RequestPrint
        );
        assert_eq!(parse_hud_action("zen"), HudAction::ToggleFullscreen);
        assert_eq!(parse_hud_action("invert"), HudAction::ToggleInvertColors);
        assert_eq!(parse_hud_action("42"), HudAction::NavigateToPage(41));
        assert_eq!(parse_hud_action(""), HudAction::None);
    }

    #[test]
    fn all_hud_commands_contain_valid_print_item() {
        let print_item = ALL_HUD_COMMANDS.iter().find(|cmd| cmd.id == "print");
        assert!(print_item.is_some());
        let print_item = print_item.unwrap();
        assert_eq!(parse_hud_action(print_item.id), HudAction::RequestPrint);
        assert_eq!(parse_hud_action(print_item.title), HudAction::RequestPrint);
    }

    #[test]
    fn hud_print_action_dispatch_avoids_refcell_borrow_panic() {
        let state = Rc::new(RefCell::new(AppState::new(
            barepdf_core::UserPreferences::default(),
        )));
        let print_called = Rc::new(AtomicBool::new(false));

        let state_print = state.clone();
        let print_called_clone = print_called.clone();
        let on_request_print = move || {
            let mut _app = state_print.borrow_mut();
            print_called_clone.store(true, Ordering::SeqCst);
        };

        let state_cmd = state.clone();
        let action = {
            let mut _app = state_cmd.borrow_mut();
            parse_hud_action("print")
        };

        if action == HudAction::RequestPrint {
            on_request_print();
        }

        assert!(print_called.load(Ordering::SeqCst));
    }

    #[test]
    fn hud_commands_all_registered_ids_execute_cleanly() {
        crate::presentation::test_support::run_on_ui_thread(|| {
            let mut app = AppState::new(barepdf_core::UserPreferences::default());
            let engine = barepdf_pdf::PdfiumEngine::new().expect("pdf engine");
            let scheduler =
                RenderScheduler::spawn(engine, barepdf_core::MemoryBudget::new(1024 * 1024));
            let window = AppWindow::new().expect("slint app window");

            for item in ALL_HUD_COMMANDS {
                let action = execute_hud_command(&mut app, &scheduler, &window, item.id);
                assert_ne!(
                    action,
                    HudAction::None,
                    "command ID '{}' failed to parse a recognized HudAction",
                    item.id
                );
                let banner = window.get_banner_text();
                assert!(
                    !banner.starts_with("Unknown command"),
                    "command ID '{}' failed to match and produced banner: {}",
                    item.id,
                    banner
                );
            }
        });
    }
}
