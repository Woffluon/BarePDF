#![allow(clippy::cast_possible_wrap, clippy::cast_precision_loss)]

use super::state::AppState;
use super::ui::{
    compute_search_highlights, compute_selection_boxes, ensure_layout, visible_page_indices,
};
use barepdf_core::{DocumentAnnotations, InkStroke, SignaturePayload, ViewingMode};
use barepdf_i18n::{t, ResolvedLanguage};
use barepdf_render::RenderKind;
use barepdf_ui::{AppWindow, PageItem, TabItem, ThumbnailItem};
use slint::{Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};

pub(crate) fn refresh_page_model(app: &mut AppState, window: &AppWindow) {
    ensure_layout(app);
    let indices = visible_page_indices(app, window);
    app.visible_page_indices.clone_from(&indices);

    let model = VecModel::default();
    for index in &indices {
        let index = *index;
        let Some(layout_page) = app.layout.pages.get(index as usize).cloned() else {
            continue;
        };
        let image = app
            .active_document()
            .and_then(|document| app.page_images.get(document, index, RenderKind::Page));
        let has_bitmap = image.is_some();
        let selection_boxes = compute_selection_boxes(
            app,
            index,
            layout_page.width as f32,
            layout_page.height as f32,
        );
        let search_highlights = compute_search_highlights(
            app,
            index,
            layout_page.width as f32,
            layout_page.height as f32,
        );
        model.push(PageItem {
            page_index: index as i32,
            page_number: SharedString::from((index + 1).to_string()),
            width: layout_page.width as f32,
            height: layout_page.height as f32,
            y_offset: layout_page.y_offset,
            bitmap: image.unwrap_or_default(),
            has_bitmap,
            selection_boxes: ModelRc::new(VecModel::from(selection_boxes)),
            search_highlights: ModelRc::new(VecModel::from(search_highlights)),
        });
    }
    window.set_visible_pages(ModelRc::new(model));

    let first_page = if app.viewing_mode == ViewingMode::TwoPageSpread {
        app.current_page & !1
    } else {
        app.current_page
    };
    if let Some(page) = app.layout.pages.get(first_page as usize) {
        window.set_page_display_width(page.width as f32);
        window.set_page_display_height(page.height as f32);
        let page_width = app
            .page_dimensions
            .get(first_page as usize)
            .map_or(app.first_page_dimensions.0, |dimensions| dimensions.0)
            .max(1.0);
        let effective_zoom = page.width as f32 / page_width;
        window.set_zoom_str(SharedString::from(format!(
            "{}%",
            (effective_zoom * 100.0).round()
        )));
    }

    if app.viewing_mode == ViewingMode::TwoPageSpread {
        let second_page = first_page + 1;
        if second_page < app.page_count() {
            window.set_has_second_spread_page(true);
            window.set_second_page_index(second_page as i32);
            if let Some(second_layout) = app.layout.pages.get(second_page as usize) {
                window.set_second_page_width(second_layout.width as f32);
                window.set_second_page_height(second_layout.height as f32);
            }
            let second_image = app
                .active_document()
                .and_then(|doc| app.page_images.get(doc, second_page, RenderKind::Page))
                .unwrap_or_default();
            window.set_second_page_image(second_image);
        } else {
            window.set_has_second_spread_page(false);
        }
    } else {
        window.set_has_second_spread_page(false);
    }

    refresh_annotation_overlays(app, window);
}

pub(crate) fn refresh_annotation_overlays(app: &AppState, window: &AppWindow) {
    let doc_annotations = app
        .active_document()
        .and_then(|doc| app.annotations.get(&doc));
    let has_unsaved = doc_annotations.is_some_and(|a| !a.is_empty());
    window.set_has_unsaved_annotations(has_unsaved);

    let first_page = if app.viewing_mode == ViewingMode::TwoPageSpread {
        app.current_page & !1
    } else {
        app.current_page
    };
    window.set_current_annotation_page_index(first_page as i32);

    let current_overlay =
        render_page_annotation_overlay(doc_annotations, app.active_stroke.as_ref(), first_page);
    window.set_current_page_has_annotation_overlay(current_overlay.size().width > 0);
    window.set_current_page_annotation_overlay(current_overlay);

    if app.viewing_mode == ViewingMode::TwoPageSpread && first_page + 1 < app.page_count() {
        let second_overlay = render_page_annotation_overlay(
            doc_annotations,
            app.active_stroke.as_ref(),
            first_page + 1,
        );
        window.set_second_page_has_annotation_overlay(second_overlay.size().width > 0);
        window.set_second_page_annotation_overlay(second_overlay);
    } else {
        window.set_second_page_has_annotation_overlay(false);
        window.set_second_page_annotation_overlay(Image::default());
    }

    if app.viewing_mode == ViewingMode::ContinuousVertical
        && (has_unsaved || app.active_stroke.is_some())
    {
        let mut overlays = Vec::with_capacity(app.page_count() as usize);
        for page_idx in 0..app.page_count() {
            overlays.push(render_page_annotation_overlay(
                doc_annotations,
                app.active_stroke.as_ref(),
                page_idx,
            ));
        }
        window.set_page_annotation_overlays(ModelRc::new(VecModel::from(overlays)));
    } else if !has_unsaved && app.active_stroke.is_none() {
        window.set_page_annotation_overlays(ModelRc::new(VecModel::default()));
    }
}

pub(crate) fn render_page_annotation_overlay(
    annotations: Option<&DocumentAnnotations>,
    active_stroke: Option<&InkStroke>,
    page: u32,
) -> Image {
    let has_page_annotations = annotations.is_some_and(|a| {
        a.highlights.iter().any(|h| h.page.get() == page)
            || a.strokes.iter().any(|s| s.page.get() == page)
            || a.signatures.iter().any(|s| s.page.get() == page)
    }) || active_stroke.is_some_and(|s| s.page.get() == page);

    if !has_page_annotations {
        return Image::default();
    }

    const W: u32 = 420;
    const H: u32 = 560;
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(W, H);
    let bytes = buffer.make_mut_bytes();

    if let Some(annotations) = annotations {
        for h in annotations
            .highlights
            .iter()
            .filter(|h| h.page.get() == page)
        {
            let x0 = ((h.x_norm * W as f32).round() as i32).clamp(0, W as i32);
            let y0 = ((h.y_norm * H as f32).round() as i32).clamp(0, H as i32);
            let x1 = (((h.x_norm + h.w_norm) * W as f32).round() as i32).clamp(0, W as i32);
            let y1 = (((h.y_norm + h.h_norm) * H as f32).round() as i32).clamp(0, H as i32);
            for y in y0..y1 {
                for x in x0..x1 {
                    blend_pixel(bytes, W, x as u32, y as u32, (250, 204, 21, 96));
                }
            }
        }

        for stroke in annotations.strokes.iter().filter(|s| s.page.get() == page) {
            draw_ink_stroke(bytes, W, H, stroke);
        }

        for sig in annotations
            .signatures
            .iter()
            .filter(|s| s.page.get() == page)
        {
            let bx = (sig.x_norm * W as f32).round() as i32;
            let by = (sig.y_norm * H as f32).round() as i32;
            let bw = (sig.w_norm * W as f32).round().max(1.0) as i32;
            let bh = (sig.h_norm * H as f32).round().max(1.0) as i32;
            match &sig.payload {
                SignaturePayload::Drawn(polylines) => {
                    for line in polylines {
                        draw_normalized_polyline_in_box(
                            bytes,
                            W,
                            H,
                            (bx, by, bw, bh),
                            line,
                            (20, 20, 40, 255),
                            1.5,
                        );
                    }
                }
                SignaturePayload::Image {
                    width,
                    height,
                    rgba,
                } => {
                    if *width > 0 && *height > 0 && rgba.len() >= (*width * *height * 4) as usize {
                        for dy in 0..bh {
                            let py = by + dy;
                            if py < 0 || py >= H as i32 {
                                continue;
                            }
                            let sy = ((dy as u32) * *height / (bh as u32).max(1))
                                .min(height.saturating_sub(1));
                            for dx in 0..bw {
                                let px = bx + dx;
                                if px < 0 || px >= W as i32 {
                                    continue;
                                }
                                let sx = ((dx as u32) * *width / (bw as u32).max(1))
                                    .min(width.saturating_sub(1));
                                let idx = ((sy * *width + sx) * 4) as usize;
                                let c = (rgba[idx], rgba[idx + 1], rgba[idx + 2], rgba[idx + 3]);
                                blend_pixel(bytes, W, px as u32, py as u32, c);
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(stroke) = active_stroke.filter(|s| s.page.get() == page) {
        draw_ink_stroke(bytes, W, H, stroke);
    }

    Image::from_rgba8(buffer)
}

pub(crate) fn render_signature_pad_preview(
    strokes: &[Vec<(f32, f32)>],
    active: Option<&Vec<(f32, f32)>>,
    uploaded: Option<&(u32, u32, Vec<u8>)>,
) -> Image {
    if let Some((w, h, rgba)) = uploaded {
        if *w > 0 && *h > 0 && rgba.len() == (*w * *h * 4) as usize {
            let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(rgba, *w, *h);
            return Image::from_rgba8(buffer);
        }
    }
    if strokes.is_empty() && active.is_none_or(Vec::is_empty) {
        return Image::default();
    }
    const W: u32 = 360;
    const H: u32 = 160;
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(W, H);
    let bytes = buffer.make_mut_bytes();
    for line in strokes {
        draw_normalized_polyline_in_box(
            bytes,
            W,
            H,
            (0, 0, W as i32, H as i32),
            line,
            (20, 20, 40, 255),
            2.0,
        );
    }
    if let Some(line) = active {
        draw_normalized_polyline_in_box(
            bytes,
            W,
            H,
            (0, 0, W as i32, H as i32),
            line,
            (20, 20, 40, 255),
            2.0,
        );
    }
    Image::from_rgba8(buffer)
}

fn draw_ink_stroke(bytes: &mut [u8], w: u32, h: u32, stroke: &InkStroke) {
    let color = stroke.color.rgba();
    let radius = (stroke.width_pts * 0.45).clamp(1.0, 6.0);
    draw_normalized_polyline_in_box(
        bytes,
        w,
        h,
        (0, 0, w as i32, h as i32),
        &stroke.points,
        color,
        radius,
    );
}

fn draw_normalized_polyline_in_box(
    bytes: &mut [u8],
    canvas_w: u32,
    canvas_h: u32,
    rect: (i32, i32, i32, i32),
    points: &[(f32, f32)],
    color: (u8, u8, u8, u8),
    radius: f32,
) {
    let (bx, by, bw, bh) = rect;
    if points.is_empty() {
        return;
    }
    if points.len() == 1 {
        let x = bx as f32 + points[0].0 * bw as f32;
        let y = by as f32 + points[0].1 * bh as f32;
        draw_disc(bytes, canvas_w, canvas_h, x, y, radius, color);
        return;
    }
    for pair in points.windows(2) {
        let x0 = bx as f32 + pair[0].0 * bw as f32;
        let y0 = by as f32 + pair[0].1 * bh as f32;
        let x1 = bx as f32 + pair[1].0 * bw as f32;
        let y1 = by as f32 + pair[1].1 * bh as f32;
        let steps = ((x1 - x0).hypot(y1 - y0).ceil() as u32).clamp(1, 512);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let x = x0 + (x1 - x0) * t;
            let y = y0 + (y1 - y0) * t;
            draw_disc(bytes, canvas_w, canvas_h, x, y, radius, color);
        }
    }
}

fn draw_disc(
    bytes: &mut [u8],
    canvas_w: u32,
    canvas_h: u32,
    cx: f32,
    cy: f32,
    radius: f32,
    color: (u8, u8, u8, u8),
) {
    let r = radius.ceil() as i32;
    let r2 = radius * radius;
    let icx = cx.round() as i32;
    let icy = cy.round() as i32;
    for dy in -r..=r {
        let py = icy + dy;
        if py < 0 || py >= canvas_h as i32 {
            continue;
        }
        for dx in -r..=r {
            let px = icx + dx;
            if px < 0 || px >= canvas_w as i32 {
                continue;
            }
            if (dx * dx + dy * dy) as f32 <= r2 {
                set_pixel(bytes, canvas_w, px as u32, py as u32, color);
            }
        }
    }
}

fn set_pixel(bytes: &mut [u8], width: u32, x: u32, y: u32, color: (u8, u8, u8, u8)) {
    let idx = ((y * width + x) * 4) as usize;
    if idx + 3 < bytes.len() {
        bytes[idx] = color.0;
        bytes[idx + 1] = color.1;
        bytes[idx + 2] = color.2;
        bytes[idx + 3] = color.3;
    }
}

fn blend_pixel(bytes: &mut [u8], width: u32, x: u32, y: u32, color: (u8, u8, u8, u8)) {
    let idx = ((y * width + x) * 4) as usize;
    if idx + 3 < bytes.len() {
        let a = u16::from(color.3);
        let inv = 255 - a;
        bytes[idx] = ((u16::from(color.0) * a + u16::from(bytes[idx]) * inv) / 255) as u8;
        bytes[idx + 1] = ((u16::from(color.1) * a + u16::from(bytes[idx + 1]) * inv) / 255) as u8;
        bytes[idx + 2] = ((u16::from(color.2) * a + u16::from(bytes[idx + 2]) * inv) / 255) as u8;
        bytes[idx + 3] = bytes[idx + 3].max(color.3);
    }
}

pub(crate) fn refresh_thumbnail_model(app: &mut AppState, window: &AppWindow) {
    let model = VecModel::default();
    for index in 0..app.page_count() {
        model.push(thumbnail_item(app, index));
    }
    window.set_thumbnail_items(ModelRc::new(model));
}

pub(crate) fn refresh_tool_thumbnails(
    window: &AppWindow,
    app: &mut AppState,
    selected_range: &str,
) {
    let selected = super::callbacks::selected_tool_pages(selected_range, app.page_count());
    let model = VecModel::default();
    for index in 0..app.page_count() {
        let (width, height) = app
            .page_dimensions
            .get(index as usize)
            .copied()
            .unwrap_or(app.first_page_dimensions);
        let display_width = 140.0;
        let display_height = (display_width * height / width.max(1.0)).min(150.0);
        let image = app.active_document().and_then(|document| {
            app.thumbnail_images
                .get(document, index, RenderKind::Thumbnail)
        });
        model.push(ThumbnailItem {
            page_index: index as i32,
            page_number: thumbnail_page_label(app.preferences.language.resolve(), index),
            width: display_width,
            height: display_height,
            bitmap: image.clone().unwrap_or_default(),
            has_bitmap: image.is_some(),
            is_selected: selected.contains(&(index + 1)),
        });
    }
    window.set_thumbnail_items(ModelRc::new(model));
}

pub(crate) fn refresh_tab_model(app: &AppState, window: &AppWindow) {
    let active = app.application.tabs.active_id();
    let items = app
        .application
        .tabs
        .tabs()
        .iter()
        .map(|tab| TabItem {
            id: i32::try_from(tab.id.get()).unwrap_or(i32::MAX),
            title: SharedString::from(tab.title.as_str()),
            is_active: active == Some(tab.id),
            is_loading: tab.is_loading(),
        })
        .collect::<Vec<_>>();
    window.set_tab_items(ModelRc::new(VecModel::from(items)));
}

#[allow(dead_code)]
pub(crate) fn refresh_bookmark_model(app: &AppState, window: &AppWindow) {
    use barepdf_ui::BookmarkItem;
    let model = VecModel::default();

    if let Some(active_tab) = app.application.tabs.active() {
        if let Some(path) = &active_tab.path {
            if let Some(session) = app.preferences.open_tabs.iter().find(|s| &s.path == path) {
                for bookmark in &session.bookmarks {
                    model.push(BookmarkItem {
                        title: SharedString::from(bookmark.title.as_str()),
                        page_index: bookmark.page_index as i32,
                        page_number: SharedString::from((bookmark.page_index + 1).to_string()),
                    });
                }
            }
        }
    }

    window.set_bookmark_items(ModelRc::new(model));
}

pub(crate) fn refresh_thumbnail_row(app: &mut AppState, window: &AppWindow, index: u32) {
    let model = window.get_thumbnail_items();
    if index < app.page_count() && (index as usize) < model.row_count() {
        model.set_row_data(index as usize, thumbnail_item(app, index));
    }
}

pub(crate) fn refresh_thumbnail_selection(
    app: &mut AppState,
    window: &AppWindow,
    previous_page: u32,
) {
    refresh_thumbnail_row(app, window, previous_page);
    if previous_page != app.current_page {
        refresh_thumbnail_row(app, window, app.current_page);
    }
}

fn thumbnail_item(app: &mut AppState, index: u32) -> ThumbnailItem {
    let (width, height) = app
        .page_dimensions
        .get(index as usize)
        .copied()
        .unwrap_or(app.first_page_dimensions);
    let display_width = 140.0;
    let display_height = (display_width * height / width.max(1.0)).min(150.0);
    let image = app.active_document().and_then(|document| {
        app.thumbnail_images
            .get(document, index, RenderKind::Thumbnail)
    });
    ThumbnailItem {
        page_index: index as i32,
        page_number: thumbnail_page_label(app.preferences.language.resolve(), index),
        width: display_width,
        height: display_height,
        bitmap: image.clone().unwrap_or_default(),
        has_bitmap: image.is_some(),
        is_selected: index == app.current_page,
    }
}

fn thumbnail_page_label(language: ResolvedLanguage, index: u32) -> SharedString {
    SharedString::from(format!("{} {}", t(language, "page.thumbnail"), index + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_page_label_uses_the_selected_language() {
        assert_eq!(
            thumbnail_page_label(ResolvedLanguage::Turkish, 1).as_str(),
            "Sayfa 2"
        );
    }
}
