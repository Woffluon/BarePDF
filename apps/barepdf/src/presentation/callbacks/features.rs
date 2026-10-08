use barepdf_core::PageIndex;
use barepdf_render::{RenderCommand, RenderScheduler};
use barepdf_ui::AppWindow;
use slint::{ComponentHandle, SharedString};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use super::super::models::{refresh_bookmark_model, refresh_page_model};
use super::super::state::AppState;
use super::super::ui::{
    invalidate_layout_and_render, navigate_to_page, persist_preferences, refresh_outline_model,
    request_visible_thumbnails, send_render_command, show_banner,
};

pub(super) fn connect_niche_feature_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    preferences_path: &Path,
) {
    let weak = window.as_weak();
    let state_tab = state.clone();
    let scheduler_tab = scheduler.clone();
    window.on_request_sidebar_tab(move |tab| {
        if let Some(window) = weak.upgrade() {
            let mut app = state_tab.borrow_mut();
            if tab == 1 && !app.in_flight.outline_requested {
                if let Some(document_id) = app.active_document() {
                    app.in_flight.outline_requested = send_render_command(
                        &mut app,
                        &scheduler_tab,
                        RenderCommand::FetchOutline { document_id },
                    );
                }
            } else if tab == 0 {
                request_visible_thumbnails(&mut app, &scheduler_tab, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_outline = state.clone();
    let scheduler_outline = scheduler.clone();
    window.on_request_toggle_outline(move |index| {
        let mut target = None;
        {
            let mut app = state_outline.borrow_mut();
            if let Some(entry) = app.flat_outline.get(index as usize).cloned() {
                if entry.has_children {
                    if !app.expanded_outline.remove(&entry.path) {
                        app.expanded_outline.insert(entry.path.clone());
                    }
                    if let Some(window) = weak.upgrade() {
                        refresh_outline_model(&mut app, &window);
                    }
                }
                target = entry.page_index;
            }
        }
        if let (Some(page), Some(window)) = (target, weak.upgrade()) {
            navigate_to_page(page, &state_outline, &scheduler_outline, &window);
        }
    });

    let weak = window.as_weak();
    let state_bookmark = state.clone();
    let scheduler_bookmark = scheduler.clone();
    window.on_bookmark_selected(move |index| {
        let page = {
            let app = state_bookmark.borrow();
            let mut target_page = None;
            if let Some(active_tab) = app.application.tabs.active() {
                if let Some(path) = &active_tab.path {
                    if let Some(session) =
                        app.preferences.open_tabs.iter().find(|s| &s.path == path)
                    {
                        if let Some(bookmark) = session.bookmarks.get(index as usize) {
                            target_page = Some(bookmark.page_index);
                        }
                    }
                }
            }
            target_page
        };
        if let (Some(page), Some(window)) = (page, weak.upgrade()) {
            navigate_to_page(page, &state_bookmark, &scheduler_bookmark, &window);
        }
    });

    let weak = window.as_weak();
    let state_toggle_bm = state.clone();
    let preferences_path_bm = preferences_path.to_path_buf();
    window.on_request_toggle_bookmark(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_toggle_bm.borrow_mut();
        let current_page = app.current_page;
        let active_path = app.application.tabs.active().and_then(|t| t.path.clone());
        let Some(path) = active_path else {
            return;
        };

        let zoom_mode = app.zoom_mode;
        let pos = if let Some(pos) = app
            .preferences
            .open_tabs
            .iter()
            .position(|s| s.path == path)
        {
            pos
        } else {
            app.preferences
                .open_tabs
                .push(barepdf_core::DocumentSession {
                    path: path.clone(),
                    page_index: current_page,
                    scroll_y: 0.0,
                    zoom_mode,
                    bookmarks: Vec::new(),
                });
            app.preferences.open_tabs.len() - 1
        };
        let session = &mut app.preferences.open_tabs[pos];

        let added = crate::controllers::bookmark_controller::toggle_bookmark(
            &mut session.bookmarks,
            barepdf_core::types::PageIndex::from_raw(current_page),
            None,
        );

        refresh_bookmark_model(&app, &window);
        persist_preferences(&app.preferences, &preferences_path_bm, Some(&window));

        let msg = if added {
            barepdf_i18n::t(app.preferences.language.resolve(), "bookmark.added")
        } else {
            barepdf_i18n::t(app.preferences.language.resolve(), "bookmark.removed")
        };
        show_banner(&window, msg, false);
    });

    let weak = window.as_weak();
    let state_search = state.clone();
    let scheduler_search = scheduler.clone();
    window.on_request_search_query(move |query_str| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_search.borrow_mut();
        let query_text = query_str.as_str().trim();
        if query_text.is_empty() {
            app.search_query = None;
            app.search_matches.clear();
            app.active_search_match = 0;
            window.set_search_has_matches(false);
            window.set_search_match_counter(SharedString::from("0 / 0"));
            refresh_page_model(&mut app, &window);
            return;
        }

        let case_sensitive = window.get_search_case_sensitive();
        let whole_word = window.get_search_whole_word();
        let query = barepdf_core::search::SearchQuery::new(
            query_text.to_string(),
            case_sensitive,
            whole_word,
        );
        let Some(query) = query else {
            app.search_query = None;
            app.search_matches.clear();
            app.active_search_match = 0;
            window.set_search_has_matches(false);
            window.set_search_match_counter(SharedString::from("0 / 0"));
            refresh_page_model(&mut app, &window);
            return;
        };

        let matches = if let Some(doc) = app.active_document() {
            let mut all_matches = Vec::new();
            let mut global_index = 0;
            for geom in app.text_geometries.in_page_order(doc) {
                let page_idx = geom.page_index.get();
                let ranges = query.find_in_geometry(&geom);
                for range in ranges {
                    let mut glyph_boxes = Vec::new();
                    let start = range.start as usize;
                    let end = range.end as usize;

                    if start <= geom.glyphs.len() && end <= geom.glyphs.len() {
                        glyph_boxes.extend_from_slice(&geom.glyphs[start..end]);
                    }

                    all_matches.push(barepdf_core::search::SearchMatch {
                        page_index: PageIndex::from_raw(page_idx),
                        match_index_in_doc: global_index,
                        char_range: range,
                        glyph_boxes,
                    });
                    global_index += 1;
                }
            }
            all_matches
        } else {
            Vec::new()
        };

        let total = matches.len();
        let has_matches = total > 0;
        app.search_query = Some(query);
        app.search_matches = matches;
        app.active_search_match = 0;

        window.set_search_has_matches(has_matches);
        window.set_search_match_counter(SharedString::from(
            crate::controllers::search_controller::match_summary(0, total),
        ));
        refresh_page_model(&mut app, &window);

        if has_matches {
            if let Some(target_match) = app.search_matches.first() {
                let target_page = target_match.page_index.get();
                drop(app);
                navigate_to_page(target_page, &state_search, &scheduler_search, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_search_next = state.clone();
    let scheduler_search_next = scheduler.clone();
    window.on_request_search_next(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_search_next.borrow_mut();
        let total = app.search_matches.len();
        if total == 0 {
            return;
        }
        app.active_search_match =
            crate::controllers::search_controller::next_match(app.active_search_match, total);
        let current = app.active_search_match;
        window.set_search_match_counter(SharedString::from(
            crate::controllers::search_controller::match_summary(current, total),
        ));
        let target_page = app.search_matches.get(current).map(|m| m.page_index.get());
        if let Some(target_page) = target_page {
            drop(app);
            navigate_to_page(
                target_page,
                &state_search_next,
                &scheduler_search_next,
                &window,
            );
        }
    });

    let weak = window.as_weak();
    let state_search_prev = state.clone();
    let scheduler_search_prev = scheduler.clone();
    window.on_request_search_prev(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_search_prev.borrow_mut();
        let total = app.search_matches.len();
        if total == 0 {
            return;
        }
        app.active_search_match =
            crate::controllers::search_controller::prev_match(app.active_search_match, total);
        let current = app.active_search_match;
        window.set_search_match_counter(SharedString::from(
            crate::controllers::search_controller::match_summary(current, total),
        ));
        let target_page = app.search_matches.get(current).map(|m| m.page_index.get());
        if let Some(target_page) = target_page {
            drop(app);
            navigate_to_page(
                target_page,
                &state_search_prev,
                &scheduler_search_prev,
                &window,
            );
        }
    });

    let weak = window.as_weak();
    window.on_request_toggle_command_palette(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let open = !window.get_command_palette_open();
        window.set_command_palette_open(open);
        if open {
            window.set_command_palette_query(slint::SharedString::from(""));
        }
    });

    let weak = window.as_weak();
    let state_cmd = state.clone();
    let scheduler_cmd = scheduler.clone();
    window.on_request_execute_command(move |query| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let action = {
            let mut app = state_cmd.borrow_mut();
            super::super::hud_commands::execute_hud_command(
                &mut app,
                &scheduler_cmd,
                &window,
                &query,
            )
        };
        match action {
            super::super::hud_commands::HudAction::RequestPrint => {
                window.invoke_request_print();
            }
            super::super::hud_commands::HudAction::None => {}
        }
    });

    let weak = window.as_weak();
    let state_sel = state.clone();
    let scheduler_sel = scheduler.clone();
    window.on_request_command_selected(move |idx| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let matching = super::super::hud_commands::filter_hud_commands(
            window.get_command_palette_query().as_str(),
        );
        let action = if let Some(item) = matching.get(idx as usize) {
            let mut app = state_sel.borrow_mut();
            super::super::hud_commands::execute_hud_command(
                &mut app,
                &scheduler_sel,
                &window,
                item.id,
            )
        } else {
            super::super::hud_commands::HudAction::None
        };
        window.set_command_palette_open(false);
        match action {
            super::super::hud_commands::HudAction::RequestPrint => {
                window.invoke_request_print();
            }
            super::super::hud_commands::HudAction::None => {}
        }
    });

    let weak = window.as_weak();
    let state_tint = state.clone();
    let scheduler_tint = scheduler.clone();
    window.on_request_set_paper_tint(move |tint| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_tint.borrow_mut();
        app.preferences.paper_tint = tint as u8;
        window.set_paper_tint(tint);
        invalidate_layout_and_render(&mut app, &scheduler_tint, &window, false);
    });

    let weak = window.as_weak();
    let state_invert = state.clone();
    let scheduler_invert = scheduler.clone();
    window.on_request_toggle_invert_colors(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_invert.borrow_mut();
        app.preferences.invert_colors = !app.preferences.invert_colors;
        scheduler_invert.set_invert_colors(app.preferences.invert_colors);
        window.set_invert_page_colors(app.preferences.invert_colors);
        invalidate_layout_and_render(&mut app, &scheduler_invert, &window, true);
    });
}
