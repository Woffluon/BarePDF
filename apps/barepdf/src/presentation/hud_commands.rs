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

pub fn handle_hud_query(
    app: &mut AppState,
    scheduler: &RenderScheduler,
    window: &AppWindow,
    query: &str,
) {
    let trimmed = query.trim();

    if let Ok(page_num) = trimmed.parse::<u32>() {
        if page_num >= 1 {
            let target_index = page_num - 1;
            crate::controllers::navigation_controller::navigate(
                crate::controllers::navigation_controller::NavigationTarget::Page(target_index),
                app,
                scheduler,
                window,
            );
            window.set_command_palette_open(false);
            return;
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
    } else if lower.contains("print") || lower.contains("yazd") {
        window.invoke_request_print();
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
}
