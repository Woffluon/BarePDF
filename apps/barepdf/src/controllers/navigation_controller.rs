use crate::presentation::models::refresh_thumbnail_selection;
use crate::presentation::state::AppState;
use crate::presentation::ui::{
    ensure_layout, refresh_generation_bound_views, request_next_dimensions_batch,
};
use barepdf_core::types::ViewingMode;
use barepdf_core::types::WindowMode;
use barepdf_render::RenderScheduler;
use barepdf_ui::AppWindow;
use slint::SharedString;

#[derive(Clone, Copy)]
pub enum NavigationTarget {
    Previous,
    Next,
    First,
    Last,
    Page(u32),
}

pub fn navigate(
    target: NavigationTarget,
    app: &mut AppState,
    scheduler: &RenderScheduler,
    window: &AppWindow,
) {
    if app.page_count() == 0 {
        return;
    }

    let previous_page = app.current_page;
    let page = match target {
        NavigationTarget::Previous => app.current_page.saturating_sub(1),
        NavigationTarget::Next => (app.current_page + 1).min(app.page_count().saturating_sub(1)),
        NavigationTarget::First => 0,
        NavigationTarget::Last => app.page_count().saturating_sub(1),
        NavigationTarget::Page(p) => p.min(app.page_count().saturating_sub(1)),
    };

    let Some(page_index) =
        crate::application::DocumentController::page_index(&app.application, page)
    else {
        return;
    };
    app.current_page = page_index.get();

    ensure_layout(app);

    if app.viewing_mode == ViewingMode::ContinuousVertical
        && app.window_mode != WindowMode::Presentation
    {
        if let Some(scroll_y) = app
            .layout
            .pages
            .get(app.current_page as usize)
            .map(|page| -page.y_offset)
        {
            app.last_scroll_y = scroll_y;
            window.set_current_scroll_y(scroll_y);
        }
    }

    app.generation = scheduler.bump_generation();
    window.set_current_page_str(SharedString::from((app.current_page + 1).to_string()));
    request_next_dimensions_batch(app, scheduler);
    refresh_generation_bound_views(app, scheduler, window);
    refresh_thumbnail_selection(app, window, previous_page);
}
