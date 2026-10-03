use crate::presentation::state::AppState;

use crate::presentation::ui::{invalidate_layout_and_render, zoom_mode_index};
use barepdf_render::RenderScheduler;
use barepdf_ui::AppWindow;

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
];

fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let needle_bytes = needle.as_bytes();
    let n = needle_bytes.len();
    if haystack.len() < n {
        return false;
    }
    haystack
        .as_bytes()
        .windows(n)
        .any(|window| window.eq_ignore_ascii_case(needle_bytes))
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudAction {
    None,
    RequestPrint,
}

pub fn parse_hud_action(query: &str) -> HudAction {
    let lower = query.trim().to_lowercase();
    if lower.contains("print") || lower.contains("yazd") {
        HudAction::RequestPrint
    } else {
        HudAction::None
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
    let trimmed = query.trim();

    if let Ok(page_num) = trimmed.parse::<u32>() {
        if page_num >= 1 {
            let target_index = page_num - 1;
            super::ui::navigate_to_page_inner(target_index, app, scheduler, window);
            window.set_command_palette_open(false);
            return HudAction::None;
        }
    }

    let lower = trimmed.to_lowercase();
    window.set_command_palette_open(false);

    if lower.contains("zen") || lower == "f11" {
        let is_zen = !window.get_zen_mode();
        window.set_zen_mode(is_zen);
        window.set_sidebar_visible(!is_zen && app.preferences.sidebar_visible);
    } else if lower.contains("invert") || lower.contains("ters") {
        app.preferences.invert_colors = !app.preferences.invert_colors;
        window.set_invert_page_colors(app.preferences.invert_colors);
        invalidate_layout_and_render(app, scheduler, window, true);
    } else if lower.contains("sepia") || lower.contains("sepya") {
        app.preferences.paper_tint = 1;
        window.set_paper_tint(1);
        invalidate_layout_and_render(app, scheduler, window, false);
    } else if lower.contains("night") || lower.contains("gece") || lower.contains("dark") {
        app.preferences.paper_tint = 2;
        window.set_paper_tint(2);
        invalidate_layout_and_render(app, scheduler, window, false);
    } else if lower.contains("amber") || lower.contains("kehribar") {
        app.preferences.paper_tint = 3;
        window.set_paper_tint(3);
        invalidate_layout_and_render(app, scheduler, window, false);
    } else if lower.contains("normal") || lower.contains("orijinal") || lower.contains("original") {
        app.preferences.paper_tint = 0;
        window.set_paper_tint(0);
        invalidate_layout_and_render(app, scheduler, window, false);
    } else if let action @ HudAction::RequestPrint = parse_hud_action(&lower) {
        return action;
    } else if lower.contains("fit width") || lower.contains("geni") {
        app.zoom_mode = barepdf_core::ZoomMode::FitWidth;
        app.preferences.zoom_mode = barepdf_core::ZoomMode::FitWidth;
        window.set_zoom_mode(zoom_mode_index(app.zoom_mode));
        invalidate_layout_and_render(app, scheduler, window, false);
    } else if lower.contains("fit page") || lower.contains("sayfa s") {
        app.zoom_mode = barepdf_core::ZoomMode::FitPage;
        app.preferences.zoom_mode = barepdf_core::ZoomMode::FitPage;
        window.set_zoom_mode(zoom_mode_index(app.zoom_mode));
        invalidate_layout_and_render(app, scheduler, window, false);
    } else if !trimmed.is_empty() {
        crate::presentation::ui::show_banner(window, format!("Unknown command: {trimmed}"), false);
    }

    HudAction::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn parse_hud_action_identifies_print_queries() {
        assert_eq!(parse_hud_action("print"), HudAction::RequestPrint);
        assert_eq!(parse_hud_action("PRINT"), HudAction::RequestPrint);
        assert_eq!(parse_hud_action("  print  "), HudAction::RequestPrint);
        assert_eq!(parse_hud_action("yazdır"), HudAction::RequestPrint);
        assert_eq!(
            parse_hud_action("Print Document (Ctrl+P)"),
            HudAction::RequestPrint
        );
        assert_eq!(parse_hud_action("zen"), HudAction::None);
        assert_eq!(parse_hud_action("invert"), HudAction::None);
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

        // Simulated print callback which acquires a mutable borrow
        let state_print = state.clone();
        let print_called_clone = print_called.clone();
        let on_request_print = move || {
            let mut _app = state_print.borrow_mut();
            print_called_clone.store(true, Ordering::SeqCst);
        };

        // When executing a command, borrow is dropped before triggering the returned action:
        let state_cmd = state.clone();
        let action = {
            let mut _app = state_cmd.borrow_mut();
            parse_hud_action("print")
        };
        // _app borrow has been dropped!
        match action {
            HudAction::RequestPrint => on_request_print(),
            HudAction::None => {}
        }

        assert!(print_called.load(Ordering::SeqCst));
    }
}
