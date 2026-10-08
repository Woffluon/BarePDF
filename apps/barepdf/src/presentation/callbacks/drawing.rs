use barepdf_core::{selection::SelectionEngine, PageIndex};
use barepdf_platform_windows::WindowsFileDialogs;
use barepdf_render::RenderScheduler;
use barepdf_ui::AppWindow;
use slint::{ComponentHandle, Image, SharedString};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use super::super::models::{
    refresh_annotation_overlays, refresh_page_model, render_signature_pad_preview,
};
use super::super::state::{AppState, BackgroundUiEvent, ERASER_RADII};
use super::super::ui::{begin_open, show_banner};
use super::print::populate_print_preview_printers;

pub(crate) fn handle_background_ui_event(
    event: BackgroundUiEvent,
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
) {
    match event {
        BackgroundUiEvent::PrintersEnumerated(printers) => {
            let mut app = state.borrow_mut();
            app.in_flight.printer_enum = false;
            app.cached_printers = printers;
            if window.get_print_preview_open() {
                populate_print_preview_printers(window, &app.cached_printers);
            }
        }
        BackgroundUiEvent::AnnotationsSaved {
            doc_id,
            output_path,
            result,
        } => {
            let lang = {
                let mut app = state.borrow_mut();
                app.in_flight.annotation_save = false;
                app.preferences.language.resolve()
            };
            match result {
                Ok(()) => {
                    {
                        let mut app = state.borrow_mut();
                        app.annotations.remove(&doc_id);
                        app.active_stroke = None;
                        app.page_images.remove_document(doc_id);
                        app.thumbnail_images.remove_document(doc_id);
                        app.text_geometries.remove_document(doc_id);
                        refresh_annotation_overlays(&mut app, window);
                    }
                    window.set_drawing_mode_active(false);
                    let name = output_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("document.pdf");
                    let msg = barepdf_i18n::t(lang, "status.saved").replace("{name}", name);
                    window.set_status_text(SharedString::from(msg.as_str()));
                    show_banner(window, msg, false);
                    begin_open(output_path, None, state, scheduler, window);
                }
                Err(err) => {
                    show_banner(window, format!("Failed to save annotations: {err}"), false);
                }
            }
        }
        BackgroundUiEvent::SignatureImageDecoded { result } => match result {
            Ok((w, h, pixels)) => {
                let mut app = state.borrow_mut();
                app.sign_uploaded_image = Some((w, h, pixels));
                let preview =
                    render_signature_pad_preview(&[], None, app.sign_uploaded_image.as_ref());
                window.set_sign_pad_preview(preview);
                window.set_sign_has_preview(true);
            }
            Err(err) => {
                show_banner(
                    window,
                    format!("Could not load signature image: {err}"),
                    false,
                );
            }
        },
    }
}

fn update_drawing_undo_redo_ui(app: &AppState, window: &AppWindow) {
    if let Some(doc_id) = app.active_document() {
        if let Some(history) = app.annotation_history.get(&doc_id) {
            window.set_drawing_can_undo(history.can_undo());
            window.set_drawing_can_redo(history.can_redo());
            return;
        }
    }
    window.set_drawing_can_undo(false);
    window.set_drawing_can_redo(false);
}

pub(super) fn connect_annotation_and_signature_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    dialogs: Arc<WindowsFileDialogs>,
) {
    {
        let app = state.borrow();
        window.set_pan_mode_active(app.pan_mode);
        window.set_drawing_eraser_size_index(app.drawing_eraser_size_index as i32);
        window.set_drawing_eraser_diameter_norm(ERASER_RADII[app.drawing_eraser_size_index] * 2.0);
        window.set_drawing_toolbar_at_bottom(app.drawing_toolbar_at_bottom);
        update_drawing_undo_redo_ui(&app, window);
    }
    let weak = window.as_weak();
    let state_ctx = state.clone();
    window.on_page_right_clicked(move |page_idx, _norm_x, _norm_y, win_x, win_y| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_ctx.borrow_mut();
        if page_idx >= 0 && app.page_count() > 0 {
            app.current_page = (page_idx as u32).min(app.page_count().saturating_sub(1));
        }
        let has_sel = app.selection.is_some_and(|s| !s.is_empty());
        window.set_context_menu_has_selection(has_sel);
        window.set_context_menu_x(win_x);
        window.set_context_menu_y(win_y);
        window.set_context_menu_open(true);
    });

    let weak = window.as_weak();
    let state_find_sel = state.clone();
    window.on_context_find_selection(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let selected_text = {
            let app = state_find_sel.borrow();
            if let (Some(selection), Some(document)) = (app.selection, app.active_document()) {
                let geometries = app.text_geometries.in_page_order(document);
                let geom_refs: Vec<&barepdf_core::types::PageTextGeometry> =
                    geometries.iter().map(AsRef::as_ref).collect();
                SelectionEngine::get_selected_text_in_page_order(&selection, &geom_refs)
            } else {
                String::new()
            }
        };
        let query = selected_text.trim().to_string();
        if !query.is_empty() {
            window.set_search_open(true);
            window.set_search_query(SharedString::from(query.as_str()));
            window.invoke_request_search_query(SharedString::from(query));
        }
    });

    let weak = window.as_weak();
    let state_hl = state.clone();
    window.on_context_highlight_selection(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_hl.borrow_mut();
        let Some(selection) = app.selection else {
            return;
        };
        if selection.is_empty() {
            return;
        }
        let Some(doc_id) = app.active_document() else {
            return;
        };
        let (first_page, last_page) = selection.start_and_end();
        let mut new_quads = Vec::new();
        for p in first_page.page.get()..=last_page.page.get() {
            let page_index = PageIndex::from_raw(p);
            let Some((start, end)) = selection.range_for_page(page_index) else {
                continue;
            };
            let (pw, ph) = app
                .page_dimensions
                .get(p as usize)
                .copied()
                .unwrap_or(app.first_page_dimensions);
            if let Some(geom) = app.text_geometries.get(doc_id, p) {
                new_quads.extend(selection_to_highlight_quads(
                    &geom, page_index, start, end, pw, ph,
                ));
            }
        }
        if !new_quads.is_empty() {
            let before = app.annotations.get(&doc_id).cloned().unwrap_or_default();
            app.annotations
                .entry(doc_id)
                .or_default()
                .highlights
                .extend(new_quads);
            let history = app.annotation_history.entry(doc_id).or_default();
            history.push_snapshot(before);
            app.selection = None;
            window.set_has_selection(false);
            app.committed_overlay_cache = None;
            update_drawing_undo_redo_ui(&app, &window);
            refresh_page_model(&mut app, &window);
        }
    });

    let weak = window.as_weak();
    let state_draw_mode = state.clone();
    window.on_toggle_drawing_mode(move || {
        if let Some(window) = weak.upgrade() {
            let next = !window.get_drawing_mode_active();
            window.set_drawing_mode_active(next);
            let app = state_draw_mode.borrow();
            window.set_pan_mode_active(app.pan_mode);
            window.set_drawing_eraser_size_index(app.drawing_eraser_size_index as i32);
            window.set_drawing_eraser_diameter_norm(
                ERASER_RADII[app.drawing_eraser_size_index] * 2.0,
            );
            window.set_drawing_toolbar_at_bottom(app.drawing_toolbar_at_bottom);
            update_drawing_undo_redo_ui(&app, &window);
        }
    });

    let weak = window.as_weak();
    let state_pan = state.clone();
    window.on_toggle_pan_mode(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_pan.borrow_mut();
            app.pan_mode = !app.pan_mode;
            window.set_pan_mode_active(app.pan_mode);
        }
    });

    let weak = window.as_weak();
    let state_eraser = state.clone();
    window.on_set_drawing_eraser(move |active| {
        let mut app = state_eraser.borrow_mut();
        app.drawing_eraser = active;
        if let Some(window) = weak.upgrade() {
            window.set_drawing_eraser_active(active);
            let idx = app.drawing_eraser_size_index;
            let radius = ERASER_RADII[idx];
            window.set_drawing_eraser_size_index(idx as i32);
            window.set_drawing_eraser_diameter_norm(radius * 2.0);
        }
    });

    let weak = window.as_weak();
    let state_eraser_size = state.clone();
    window.on_set_drawing_eraser_size(move |idx| {
        let mut app = state_eraser_size.borrow_mut();
        let idx = (idx.max(0) as usize).min(ERASER_RADII.len() - 1);
        app.drawing_eraser_size_index = idx;
        let radius = ERASER_RADII[idx];
        if let Some(window) = weak.upgrade() {
            window.set_drawing_eraser_size_index(idx as i32);
            window.set_drawing_eraser_diameter_norm(radius * 2.0);
        }
    });

    let weak = window.as_weak();
    let state_toolbar_pos = state.clone();
    window.on_toggle_drawing_toolbar_position(move || {
        if let Some(window) = weak.upgrade() {
            let mut app = state_toolbar_pos.borrow_mut();
            app.drawing_toolbar_at_bottom = !app.drawing_toolbar_at_bottom;
            window.set_drawing_toolbar_at_bottom(app.drawing_toolbar_at_bottom);
        }
    });

    let weak = window.as_weak();
    let state_color = state.clone();
    window.on_set_drawing_color(move |idx| {
        let mut app = state_color.borrow_mut();
        app.drawing_color = match idx {
            1 => barepdf_core::InkColor::Red,
            2 => barepdf_core::InkColor::Blue,
            3 => barepdf_core::InkColor::Yellow,
            _ => barepdf_core::InkColor::Black,
        };
        app.drawing_eraser = false;
        if let Some(window) = weak.upgrade() {
            window.set_drawing_color_index(idx.clamp(0, 3));
            window.set_drawing_eraser_active(false);
        }
    });

    let weak = window.as_weak();
    let state_width = state.clone();
    window.on_set_drawing_width(move |idx| {
        let mut app = state_width.borrow_mut();
        app.drawing_width_pts = match idx {
            0 => 2.0,
            1 => 4.0,
            _ => 8.0,
        };
        if let Some(window) = weak.upgrade() {
            window.set_drawing_width_index(idx.clamp(0, 2));
        }
    });

    let weak = window.as_weak();
    let state_draw_down = state.clone();
    window.on_drawing_pointer_down(move |page, nx, ny| {
        if page < 0 {
            return;
        }
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_draw_down.borrow_mut();
        let Some(doc_id) = app.active_document() else {
            return;
        };
        let page_idx = PageIndex::from_raw(page as u32);
        let pt = (nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0));
        if app.drawing_eraser {
            app.last_eraser_point = Some((page_idx, pt.0, pt.1));
            let radius = ERASER_RADII[app.drawing_eraser_size_index];
            let mut modified = false;
            let mut before = None;
            if let Some(ann) = app.annotations.get_mut(&doc_id) {
                before = Some(ann.clone());
                modified = barepdf_core::erase_ink_strokes_along_segment(
                    &mut ann.strokes,
                    page_idx,
                    pt,
                    pt,
                    radius,
                    radius,
                );
            }
            if let Some(before) = before {
                let history = app.annotation_history.entry(doc_id).or_default();
                history.push_snapshot(before);
            }
            if modified {
                app.committed_overlay_cache = None;
                update_drawing_undo_redo_ui(&app, &window);
                refresh_annotation_overlays(&mut app, &window);
            }
        } else {
            app.active_stroke = Some(barepdf_core::InkStroke {
                page: page_idx,
                points: vec![pt],
                color: app.drawing_color,
                width_pts: app.drawing_width_pts,
            });
            refresh_annotation_overlays(&mut app, &window);
        }
    });

    let weak = window.as_weak();
    let state_draw_move = state.clone();
    window.on_drawing_pointer_move(move |page, nx, ny| {
        if page < 0 {
            return;
        }
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_draw_move.borrow_mut();
        let Some(doc_id) = app.active_document() else {
            return;
        };
        let page_idx = PageIndex::from_raw(page as u32);
        let pt = (nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0));
        if app.drawing_eraser {
            let prev_pt = app
                .last_eraser_point
                .filter(|(p, _, _)| *p == page_idx)
                .map(|(_, x, y)| (x, y))
                .unwrap_or(pt);
            app.last_eraser_point = Some((page_idx, pt.0, pt.1));
            let radius = ERASER_RADII[app.drawing_eraser_size_index];
            if let Some(ann) = app.annotations.get_mut(&doc_id) {
                let modified = barepdf_core::erase_ink_strokes_along_segment(
                    &mut ann.strokes,
                    page_idx,
                    prev_pt,
                    pt,
                    radius,
                    radius,
                );
                if modified {
                    app.committed_overlay_cache = None;
                    update_drawing_undo_redo_ui(&app, &window);
                    refresh_annotation_overlays(&mut app, &window);
                }
            }
        } else if let Some(stroke) = app.active_stroke.as_mut() {
            if stroke.page == page_idx && stroke.points.len() < 4096 {
                stroke.points.push(pt);
                refresh_annotation_overlays(&mut app, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_draw_up = state.clone();
    window.on_drawing_pointer_up(move |_page, nx, ny| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_draw_up.borrow_mut();
        let Some(doc_id) = app.active_document() else {
            return;
        };
        app.last_eraser_point = None;
        if app.drawing_eraser {
            let current_ann = app.annotations.get(&doc_id).cloned();
            if let Some(ann) = current_ann {
                if let Some(history) = app.annotation_history.get_mut(&doc_id) {
                    if history.undo_stack.last() == Some(&ann) {
                        history.undo_stack.pop();
                    }
                }
            }
            update_drawing_undo_redo_ui(&app, &window);
            return;
        }
        if let Some(mut stroke) = app.active_stroke.take() {
            if stroke.points.is_empty() {
                stroke.points.push((nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0)));
            }
            stroke.points = barepdf_core::smooth_ink_points(&stroke.points);
            let ann = app.annotations.entry(doc_id).or_default();
            let before = ann.clone();
            ann.strokes.push(stroke);
            let history = app.annotation_history.entry(doc_id).or_default();
            history.push_snapshot(before);
            app.committed_overlay_cache = None;
            update_drawing_undo_redo_ui(&app, &window);
            refresh_annotation_overlays(&mut app, &window);
        }
    });

    let weak = window.as_weak();
    let state_undo = state.clone();
    window.on_drawing_undo(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_undo.borrow_mut();
        let Some(doc_id) = app.active_document() else {
            return;
        };
        let current_ann = app.annotations.get(&doc_id).cloned().unwrap_or_default();
        if let Some(history) = app.annotation_history.get_mut(&doc_id) {
            if let Some(prev) = history.undo(current_ann) {
                app.annotations.insert(doc_id, prev);
                app.committed_overlay_cache = None;
                update_drawing_undo_redo_ui(&app, &window);
                refresh_annotation_overlays(&mut app, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_redo = state.clone();
    window.on_drawing_redo(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_redo.borrow_mut();
        let Some(doc_id) = app.active_document() else {
            return;
        };
        let current_ann = app.annotations.get(&doc_id).cloned().unwrap_or_default();
        if let Some(history) = app.annotation_history.get_mut(&doc_id) {
            if let Some(next) = history.redo(current_ann) {
                app.annotations.insert(doc_id, next);
                app.committed_overlay_cache = None;
                update_drawing_undo_redo_ui(&app, &window);
                refresh_annotation_overlays(&mut app, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_clear_page = state.clone();
    window.on_drawing_clear_page(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_clear_page.borrow_mut();
        let current_page = app.current_page;
        let Some(doc_id) = app.active_document() else {
            return;
        };
        if let Some(ann) = app.annotations.get_mut(&doc_id) {
            let before = ann.clone();
            ann.strokes.retain(|s| s.page.get() != current_page);
            ann.highlights.retain(|h| h.page.get() != current_page);
            ann.signatures.retain(|s| s.page.get() != current_page);
            if *ann != before {
                let history = app.annotation_history.entry(doc_id).or_default();
                history.push_snapshot(before);
                app.committed_overlay_cache = None;
                update_drawing_undo_redo_ui(&app, &window);
                refresh_annotation_overlays(&mut app, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_discard = state.clone();
    window.on_discard_annotations(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_discard.borrow_mut();
        if let Some(doc_id) = app.active_document() {
            app.annotations.remove(&doc_id);
            app.annotation_history.remove(&doc_id);
        }
        app.active_stroke = None;
        app.committed_overlay_cache = None;
        window.set_drawing_mode_active(false);
        update_drawing_undo_redo_ui(&app, &window);
        refresh_annotation_overlays(&mut app, &window);
    });

    let weak = window.as_weak();
    let state_save = state.clone();
    let scheduler_save = scheduler.clone();
    window.on_save_annotations(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        save_active_annotations(&state_save, &scheduler_save, &window, None);
    });

    let weak = window.as_weak();
    let state_save_as = state.clone();
    let scheduler_save_as = scheduler.clone();
    let dialogs_save_as = dialogs.clone();
    window.on_save_annotations_as(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let default_name = {
            let app = state_save_as.borrow();
            let Some(doc) = app.application.ready_document() else {
                return;
            };
            format!(
                "{}_annotated.pdf",
                doc.path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("document")
            )
        };
        let Some(output_path) = dialogs_save_as.save_file(&default_name) else {
            return;
        };
        save_active_annotations(
            &state_save_as,
            &scheduler_save_as,
            &window,
            Some(output_path),
        );
    });

    let weak = window.as_weak();
    let state_open_sign = state.clone();
    window.on_open_sign_modal(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_open_sign.borrow_mut();
        app.sign_pad_strokes.clear();
        app.sign_pad_active_stroke = None;
        app.sign_uploaded_image = None;
        window.set_sign_pad_preview(Image::default());
        window.set_sign_has_preview(false);
        window.set_sign_tab_index(0);
        window.set_sign_modal_open(true);
    });

    let weak = window.as_weak();
    let state_sign_down = state.clone();
    window.on_sign_pad_pointer_down(move |nx, ny| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_sign_down.borrow_mut();
        app.sign_uploaded_image = None;
        app.sign_pad_active_stroke = Some(vec![(nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0))]);
        let preview = render_signature_pad_preview(
            &app.sign_pad_strokes,
            app.sign_pad_active_stroke.as_ref(),
            None,
        );
        window.set_sign_pad_preview(preview);
        window.set_sign_has_preview(true);
    });

    let weak = window.as_weak();
    let state_sign_move = state.clone();
    window.on_sign_pad_pointer_move(move |nx, ny| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_sign_move.borrow_mut();
        if let Some(stroke) = app.sign_pad_active_stroke.as_mut() {
            if stroke.len() < 2048 {
                stroke.push((nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0)));
                let preview = render_signature_pad_preview(
                    &app.sign_pad_strokes,
                    app.sign_pad_active_stroke.as_ref(),
                    None,
                );
                window.set_sign_pad_preview(preview);
            }
        }
    });

    let weak = window.as_weak();
    let state_sign_up = state.clone();
    window.on_sign_pad_pointer_up(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_sign_up.borrow_mut();
        if let Some(stroke) = app.sign_pad_active_stroke.take() {
            if !stroke.is_empty() {
                app.sign_pad_strokes.push(stroke);
            }
            let preview = render_signature_pad_preview(&app.sign_pad_strokes, None, None);
            window.set_sign_pad_preview(preview);
            window.set_sign_has_preview(!app.sign_pad_strokes.is_empty());
        }
    });

    let weak = window.as_weak();
    let state_sign_clear = state.clone();
    window.on_sign_pad_clear(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_sign_clear.borrow_mut();
        app.sign_pad_strokes.clear();
        app.sign_pad_active_stroke = None;
        app.sign_uploaded_image = None;
        window.set_sign_pad_preview(Image::default());
        window.set_sign_has_preview(false);
    });

    let weak = window.as_weak();
    let state_sign_img = state.clone();
    let dialogs_sign_img = dialogs;
    window.on_sign_pick_image(move || {
        if weak.upgrade().is_none() {
            return;
        }
        let Some(path) = dialogs_sign_img.pick_image_file() else {
            return;
        };
        state_sign_img.borrow_mut().spawn_background_io(move || {
            let result = barepdf_platform_windows::decode_image_rgba(&path)
                .map(|bmp| bmp.into_parts())
                .map_err(|err| err.to_string());
            BackgroundUiEvent::SignatureImageDecoded { result }
        });
    });

    let weak = window.as_weak();
    let state_sign_place = state.clone();
    window.on_sign_start_placement(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let current_page = state_sign_place.borrow().current_page;
        window.set_sign_modal_open(false);
        window.set_signature_placement_page(current_page as i32);
        window.set_signature_box_x(0.35);
        window.set_signature_box_y(0.72);
        window.set_signature_box_w(0.28);
        window.set_signature_box_h(0.10);
        window.set_signature_placement_active(true);
    });

    let weak = window.as_weak();
    let state_sign_apply = state.clone();
    window.on_signature_apply(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_sign_apply.borrow_mut();
        let Some(doc_id) = app.active_document() else {
            return;
        };
        let payload = if let Some((width, height, rgba)) = app.sign_uploaded_image.clone() {
            barepdf_core::SignaturePayload::Image {
                width,
                height,
                rgba,
            }
        } else if !app.sign_pad_strokes.is_empty() {
            barepdf_core::SignaturePayload::Drawn(app.sign_pad_strokes.clone())
        } else {
            window.set_signature_placement_active(false);
            return;
        };
        let page = window.get_signature_placement_page().max(0) as u32;
        let stamp = barepdf_core::SignatureStamp {
            page: PageIndex::from_raw(page),
            x_norm: window.get_signature_box_x().clamp(0.0, 0.95),
            y_norm: window.get_signature_box_y().clamp(0.0, 0.95),
            w_norm: window.get_signature_box_w().clamp(0.05, 1.0),
            h_norm: window.get_signature_box_h().clamp(0.03, 1.0),
            payload,
        };
        let ann = app.annotations.entry(doc_id).or_default();
        let before = ann.clone();
        ann.signatures.push(stamp);
        let history = app.annotation_history.entry(doc_id).or_default();
        history.push_snapshot(before);
        app.committed_overlay_cache = None;
        window.set_signature_placement_active(false);
        update_drawing_undo_redo_ui(&app, &window);
        refresh_annotation_overlays(&mut app, &window);
    });

    let weak = window.as_weak();
    window.on_signature_cancel(move || {
        if let Some(window) = weak.upgrade() {
            window.set_signature_placement_active(false);
        }
    });
}

fn save_active_annotations(
    state: &Rc<RefCell<AppState>>,
    _scheduler: &Rc<RenderScheduler>,
    _window: &AppWindow,
    save_as_path: Option<PathBuf>,
) {
    let (doc_id, source_path, annotations) = {
        let mut app = state.borrow_mut();
        if app.in_flight.annotation_save {
            return;
        }
        let Some(doc) = app.application.ready_document() else {
            return;
        };
        let doc_id = doc.id();
        let Some(annotations) = app.annotations.get(&doc_id).cloned() else {
            return;
        };
        if annotations.is_empty() {
            return;
        }
        let source_path = doc.path().to_path_buf();
        app.in_flight.annotation_save = true;
        (doc_id, source_path, annotations)
    };
    let output_path = save_as_path.unwrap_or_else(|| source_path.clone());
    state.borrow_mut().spawn_background_io(move || {
        let result = barepdf_pdf::PdfOperations::save_with_annotations(
            &source_path,
            &annotations,
            &output_path,
        )
        .map_err(|err| err.to_string());
        BackgroundUiEvent::AnnotationsSaved {
            doc_id,
            output_path,
            result,
        }
    });
}

pub(super) fn selection_to_highlight_quads(
    geometry: &barepdf_core::PageTextGeometry,
    page_index: PageIndex,
    start: u32,
    end: u32,
    page_width: f32,
    page_height: f32,
) -> Vec<barepdf_core::HighlightQuad> {
    let pw = page_width.max(1.0);
    let ph = page_height.max(1.0);
    let start = (start as usize).min(geometry.glyphs.len());
    let end = (end as usize).min(geometry.glyphs.len());
    let mut quads: Vec<barepdf_core::HighlightQuad> = Vec::new();

    for glyph in &geometry.glyphs[start..end] {
        if glyph.width <= 0.0 || glyph.height <= 0.0 {
            continue;
        }
        let x_norm = (glyph.x / pw).clamp(0.0, 1.0);
        let y_norm = ((ph - glyph.y - glyph.height) / ph).clamp(0.0, 1.0);
        let w_norm = (glyph.width / pw).clamp(0.001, 1.0 - x_norm);
        let h_norm = (glyph.height / ph).clamp(0.001, 1.0 - y_norm);

        if let Some(last) = quads.last_mut() {
            if (last.y_norm - y_norm).abs() < 0.015
                && (last.h_norm - h_norm).abs() < 0.015
                && (x_norm - (last.x_norm + last.w_norm)).abs() < 0.025
            {
                let new_right = (x_norm + w_norm).max(last.x_norm + last.w_norm);
                last.w_norm = (new_right - last.x_norm).clamp(0.001, 1.0 - last.x_norm);
                continue;
            }
        }
        quads.push(barepdf_core::HighlightQuad {
            page: page_index,
            x_norm,
            y_norm,
            w_norm,
            h_norm,
        });
    }
    quads
}

#[cfg(test)]
pub(super) fn erase_strokes_near(
    strokes: &mut Vec<barepdf_core::InkStroke>,
    page: PageIndex,
    nx: f32,
    ny: f32,
    radius: f32,
) -> bool {
    let r2 = radius * radius;
    let before = strokes.len();
    strokes.retain(|stroke| {
        if stroke.page != page {
            return true;
        }
        !stroke
            .points
            .iter()
            .any(|&(px, py)| (px - nx) * (px - nx) + (py - ny) * (py - ny) <= r2)
    });
    strokes.len() != before
}
