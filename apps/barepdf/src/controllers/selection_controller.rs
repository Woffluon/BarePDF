use crate::presentation::state::AppState;
use crate::presentation::ui::pointer_to_pdf;
use barepdf_core::selection::SelectionEngine;
use barepdf_core::types::{TextPosition, TextSelection};
use barepdf_render::RenderCommand;
use barepdf_render::RenderScheduler;
use std::time::{Duration, Instant};

pub fn start_selection(app: &mut AppState, scheduler: &RenderScheduler, page: u32, x: f32, y: f32) {
    let Some(page_index) =
        crate::application::DocumentController::page_index(&app.application, page)
    else {
        return;
    };
    if let Some(document_id) = app.active_document() {
        if !app.text_geometries.contains_key(document_id, page) {
            let generation = app.generation;
            let _ = scheduler.send_command(RenderCommand::FetchTextGeometry {
                document_id,
                generation,
                page_index,
            });
        }
    }

    let (pdf_x, pdf_y) = pointer_to_pdf(app, page, x, y);
    let now = Instant::now();
    app.click_count = if now.duration_since(app.last_click_time) < Duration::from_millis(400) {
        app.click_count + 1
    } else {
        1
    };
    app.last_click_time = now;
    let click_count = app.click_count;

    let geometry = app
        .active_document()
        .and_then(|document| app.text_geometries.get(document, page).cloned());

    if let Some(geometry) = geometry.as_ref() {
        let character = SelectionEngine::hit_test(geometry, pdf_x, pdf_y);
        app.selection = Some(match click_count {
            2 => SelectionEngine::select_word(geometry, page_index, character),
            count if count >= 3 => SelectionEngine::select_line(geometry, page_index, character),
            _ => {
                app.is_selecting = true;
                TextSelection::new(
                    TextPosition::new(page_index, character),
                    TextPosition::new(page_index, character),
                )
            }
        });
    } else {
        app.is_selecting = true;
        app.selection = Some(TextSelection::new(
            TextPosition::new(page_index, 0),
            TextPosition::new(page_index, 0),
        ));
    }
}

pub fn update_selection(app: &mut AppState, page: u32, x: f32, y: f32) -> bool {
    if !app.is_selecting {
        return false;
    }
    let Some(page_index) =
        crate::application::DocumentController::page_index(&app.application, page)
    else {
        return false;
    };
    let Some(document) = app.active_document() else {
        return false;
    };

    let (pdf_x, pdf_y) = pointer_to_pdf(app, page, x, y);
    let character = app
        .text_geometries
        .get(document, page)
        .map(|geometry| SelectionEngine::hit_test(geometry, pdf_x, pdf_y))
        .unwrap_or(0);

    if let Some(selection) = app.selection.as_mut() {
        selection.focus = TextPosition::new(page_index, character);
    }
    true
}

pub fn clear_selection(app: &mut AppState) {
    app.selection = None;
}

pub fn finish_selection(app: &mut AppState) {
    app.is_selecting = false;
}

pub fn get_selected_text(app: &AppState) -> Option<String> {
    if let (Some(selection), Some(document)) = (app.selection.as_ref(), app.active_document()) {
        let geometries = app.text_geometries.in_page_order(document);
        let text = SelectionEngine::get_selected_text_in_page_order(selection, &geometries);
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}
