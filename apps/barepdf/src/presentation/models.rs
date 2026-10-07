#![allow(clippy::cast_possible_wrap, clippy::cast_precision_loss)]

use super::state::{AppState, CommittedOverlayCache};
use super::ui::{
    compute_search_highlights, compute_selection_boxes, ensure_layout, visible_page_indices,
};
use barepdf_core::{InkStroke, SignaturePayload, ViewingMode};
use barepdf_i18n::{t, ResolvedLanguage};
use barepdf_render::RenderKind;
use barepdf_ui::{AppWindow, PageItem, TabItem, ThumbnailItem};
use slint::{Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};

pub(crate) const PAGE_ANNOTATION_BUDGET: usize = 8 * 1024 * 1024;

pub(crate) fn compute_overlay_dimensions(page_w_pt: f32, page_h_pt: f32) -> (u32, u32) {
    let pw = page_w_pt.max(1.0);
    let ph = page_h_pt.max(1.0);
    let (mut w, mut h) = if pw <= ph {
        let w = 1200.0_f32;
        let ratio = ph / pw;
        let h = if (ratio - std::f32::consts::SQRT_2).abs() < 0.002 {
            1697.0
        } else {
            (w * ratio).round()
        };
        (w, h)
    } else {
        let h = 1200.0_f32;
        let ratio = pw / ph;
        let w = if (ratio - std::f32::consts::SQRT_2).abs() < 0.002 {
            1697.0
        } else {
            (h * ratio).round()
        };
        (w, h)
    };
    let max_pixels = (PAGE_ANNOTATION_BUDGET / 4) as f32;
    let total_pixels = w * h;
    if total_pixels > max_pixels {
        let scale = (max_pixels / total_pixels).sqrt();
        w = (w * scale).floor().max(1.0);
        h = (h * scale).floor().max(1.0);
    }
    (w as u32, h as u32)
}

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

    if let Some(doc) = app.active_document() {
        if let Some(img) = app.page_images.get(doc, first_page, RenderKind::Page) {
            window.set_page_bitmap(img);
        }
    }

    refresh_annotation_overlays(app, window);
}

pub(crate) fn refresh_annotation_overlays(app: &mut AppState, window: &AppWindow) {
    let doc_annotations = app
        .active_document()
        .and_then(|doc| app.annotations.get(&doc));
    let has_unsaved = doc_annotations.is_some_and(|a| !a.is_empty());
    window.set_has_unsaved_annotations(has_unsaved);

    if let Some(stroke) = app.active_stroke.clone() {
        let stroke_page = stroke.page.get();
        window.set_current_annotation_page_index(stroke_page as i32);
        let overlay = render_page_annotation_overlay(app, stroke_page, Some(&stroke));
        window.set_current_page_has_annotation_overlay(overlay.size().width > 0);
        window.set_current_page_annotation_overlay(overlay);

        if app.viewing_mode == ViewingMode::TwoPageSpread {
            let first_page = app.current_page & !1;
            let other_page = if stroke_page == first_page {
                first_page + 1
            } else {
                first_page
            };
            if other_page < app.page_count() {
                let second_overlay = render_page_annotation_overlay(app, other_page, None);
                window.set_second_page_has_annotation_overlay(second_overlay.size().width > 0);
                window.set_second_page_annotation_overlay(second_overlay);
            } else {
                window.set_second_page_has_annotation_overlay(false);
                window.set_second_page_annotation_overlay(Image::default());
            }
        }
        return;
    }

    let first_page = if app.viewing_mode == ViewingMode::TwoPageSpread {
        app.current_page & !1
    } else {
        app.current_page
    };
    window.set_current_annotation_page_index(first_page as i32);

    let current_overlay = render_page_annotation_overlay(app, first_page, None);
    window.set_current_page_has_annotation_overlay(current_overlay.size().width > 0);
    window.set_current_page_annotation_overlay(current_overlay);

    if app.viewing_mode == ViewingMode::TwoPageSpread && first_page + 1 < app.page_count() {
        let second_overlay = render_page_annotation_overlay(app, first_page + 1, None);
        window.set_second_page_has_annotation_overlay(second_overlay.size().width > 0);
        window.set_second_page_annotation_overlay(second_overlay);
    } else {
        window.set_second_page_has_annotation_overlay(false);
        window.set_second_page_annotation_overlay(Image::default());
    }

    if app.viewing_mode == ViewingMode::ContinuousVertical && has_unsaved {
        let mut overlays = Vec::with_capacity(app.page_count() as usize);
        for page_idx in 0..app.page_count() {
            overlays.push(render_page_annotation_overlay(app, page_idx, None));
        }
        window.set_page_annotation_overlays(ModelRc::new(VecModel::from(overlays)));
    } else if !has_unsaved {
        window.set_page_annotation_overlays(ModelRc::new(VecModel::default()));
    }
}

pub(crate) fn render_page_annotation_overlay(
    app: &mut AppState,
    page: u32,
    active_stroke: Option<&InkStroke>,
) -> Image {
    let doc_id = app.active_document();
    let doc_annotations = doc_id.and_then(|doc| app.annotations.get(&doc));
    let has_page_annotations = doc_annotations.is_some_and(|a| {
        a.highlights.iter().any(|h| h.page.get() == page)
            || a.strokes.iter().any(|s| s.page.get() == page)
            || a.signatures.iter().any(|s| s.page.get() == page)
    }) || active_stroke.is_some_and(|s| s.page.get() == page);

    if !has_page_annotations {
        return Image::default();
    }

    let (pw, ph) = app
        .page_dimensions
        .get(page as usize)
        .copied()
        .unwrap_or(app.first_page_dimensions);
    let (w, h) = compute_overlay_dimensions(pw, ph);

    let doc_id = doc_id.unwrap_or_else(|| barepdf_core::DocumentId::new(1));

    let can_use_cache = active_stroke.is_some()
        && app.committed_overlay_cache.as_ref().is_some_and(|c| {
            c.document_id == doc_id && c.page == page && c.width == w && c.height == h
        });

    if can_use_cache {
        let cached = app.committed_overlay_cache.as_ref().unwrap();
        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&cached.pixels, w, h);
        let bytes = buffer.make_mut_bytes();
        if let Some(stroke) = active_stroke.filter(|s| s.page.get() == page) {
            draw_ink_stroke(bytes, w, h, stroke);
        }
        return Image::from_rgba8(buffer);
    }

    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(w, h);
    let bytes = buffer.make_mut_bytes();

    if let Some(annotations) = doc_annotations {
        for h_rect in annotations
            .highlights
            .iter()
            .filter(|h_rect| h_rect.page.get() == page)
        {
            let x0 = ((h_rect.x_norm * w as f32).round() as i32).clamp(0, w as i32);
            let y0 = ((h_rect.y_norm * h as f32).round() as i32).clamp(0, h as i32);
            let x1 =
                (((h_rect.x_norm + h_rect.w_norm) * w as f32).round() as i32).clamp(0, w as i32);
            let y1 =
                (((h_rect.y_norm + h_rect.h_norm) * h as f32).round() as i32).clamp(0, h as i32);
            for y in y0..y1 {
                for x in x0..x1 {
                    blend_pixel_safe(bytes, w, x as u32, y as u32, (250, 204, 21, 96));
                }
            }
        }

        for stroke in annotations.strokes.iter().filter(|s| s.page.get() == page) {
            draw_ink_stroke(bytes, w, h, stroke);
        }

        for sig in annotations
            .signatures
            .iter()
            .filter(|s| s.page.get() == page)
        {
            let bx = (sig.x_norm * w as f32).round() as i32;
            let by = (sig.y_norm * h as f32).round() as i32;
            let bw = (sig.w_norm * w as f32).round().max(1.0) as i32;
            let bh = (sig.h_norm * h as f32).round().max(1.0) as i32;
            match &sig.payload {
                SignaturePayload::Drawn(polylines) => {
                    for line in polylines {
                        draw_normalized_polyline_in_box(
                            bytes,
                            w,
                            h,
                            (bx, by, bw, bh),
                            line,
                            (20, 20, 40, 255),
                            2.5,
                        );
                    }
                }
                SignaturePayload::Image {
                    width: img_w,
                    height: img_h,
                    rgba,
                } => {
                    if *img_w > 0 && *img_h > 0 && rgba.len() >= (*img_w * *img_h * 4) as usize {
                        for dy in 0..bh {
                            let py = by + dy;
                            if py < 0 || py >= h as i32 {
                                continue;
                            }
                            let sy = ((dy as u32) * *img_h / (bh as u32).max(1))
                                .min(img_h.saturating_sub(1));
                            for dx in 0..bw {
                                let px = bx + dx;
                                if px < 0 || px >= w as i32 {
                                    continue;
                                }
                                let sx = ((dx as u32) * *img_w / (bw as u32).max(1))
                                    .min(img_w.saturating_sub(1));
                                let idx = ((sy * *img_w + sx) * 4) as usize;
                                let c = (rgba[idx], rgba[idx + 1], rgba[idx + 2], rgba[idx + 3]);
                                blend_pixel_safe(bytes, w, px as u32, py as u32, c);
                            }
                        }
                    }
                }
            }
        }
    }

    if active_stroke.is_some() {
        app.committed_overlay_cache = Some(CommittedOverlayCache {
            document_id: doc_id,
            page,
            width: w,
            height: h,
            pixels: bytes.to_vec(),
        });
        if let Some(stroke) = active_stroke.filter(|s| s.page.get() == page) {
            draw_ink_stroke(bytes, w, h, stroke);
        }
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
    let scale = (w as f32 / 600.0).max(0.5);
    let radius = (stroke.width_pts * scale * 0.45).clamp(1.0, 32.0);
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
        draw_capsule_segment_aa(bytes, canvas_w, canvas_h, (x, y), (x, y), radius, color);
        return;
    }
    for pair in points.windows(2) {
        let x0 = bx as f32 + pair[0].0 * bw as f32;
        let y0 = by as f32 + pair[0].1 * bh as f32;
        let x1 = bx as f32 + pair[1].0 * bw as f32;
        let y1 = by as f32 + pair[1].1 * bh as f32;
        draw_capsule_segment_aa(bytes, canvas_w, canvas_h, (x0, y0), (x1, y1), radius, color);
    }
}

pub(crate) fn draw_capsule_segment_aa(
    bytes: &mut [u8],
    canvas_w: u32,
    canvas_h: u32,
    p0: (f32, f32),
    p1: (f32, f32),
    radius: f32,
    color: (u8, u8, u8, u8),
) {
    if canvas_w == 0 || canvas_h == 0 || radius <= 0.0 || color.3 == 0 {
        return;
    }
    let r_pad = radius + 1.5;
    let min_x = (p0.0.min(p1.0) - r_pad).floor().max(0.0) as u32;
    let max_x = (p0.0.max(p1.0) + r_pad).ceil().min(canvas_w as f32 - 1.0) as u32;
    let min_y = (p0.1.min(p1.1) - r_pad).floor().max(0.0) as u32;
    let max_y = (p0.1.max(p1.1) + r_pad).ceil().min(canvas_h as f32 - 1.0) as u32;

    let vx = p1.0 - p0.0;
    let vy = p1.1 - p0.1;
    let seg_len_sq = vx * vx + vy * vy;

    for py in min_y..=max_y {
        let y = py as f32 + 0.5;
        for px in min_x..=max_x {
            let x = px as f32 + 0.5;
            let dist = if seg_len_sq < 1e-6 {
                (x - p0.0).hypot(y - p0.1)
            } else {
                let t = (((x - p0.0) * vx + (y - p0.1) * vy) / seg_len_sq).clamp(0.0, 1.0);
                let proj_x = p0.0 + t * vx;
                let proj_y = p0.1 + t * vy;
                (x - proj_x).hypot(y - proj_y)
            };

            let coverage = (radius + 0.5 - dist).clamp(0.0, 1.0);
            if coverage > 0.0 {
                let pixel_color = if coverage >= 1.0 {
                    color
                } else {
                    let a = (color.3 as f32 * coverage).round() as u8;
                    (color.0, color.1, color.2, a)
                };
                blend_pixel_safe(bytes, canvas_w, px, py, pixel_color);
            }
        }
    }
}

pub(crate) fn blend_pixel_safe(
    bytes: &mut [u8],
    width: u32,
    x: u32,
    y: u32,
    color: (u8, u8, u8, u8),
) {
    let idx = ((y * width + x) * 4) as usize;
    if idx + 3 >= bytes.len() || color.3 == 0 {
        return;
    }
    let dst_a = bytes[idx + 3];
    if dst_a == 0 {
        bytes[idx] = color.0;
        bytes[idx + 1] = color.1;
        bytes[idx + 2] = color.2;
        bytes[idx + 3] = color.3;
        return;
    }
    if color.3 == 255 {
        bytes[idx] = color.0;
        bytes[idx + 1] = color.1;
        bytes[idx + 2] = color.2;
        bytes[idx + 3] = 255;
        return;
    }
    let sa = color.3 as f32 / 255.0;
    let da = dst_a as f32 / 255.0;
    let inv_sa = 1.0 - sa;
    let out_a = sa + da * inv_sa;
    if out_a <= 0.0 {
        return;
    }
    let blend_c = |sc: u8, dc: u8| -> u8 {
        let c = ((sc as f32 * sa + dc as f32 * da * inv_sa) / out_a).round();
        c.clamp(0.0, 255.0) as u8
    };
    bytes[idx] = blend_c(color.0, bytes[idx]);
    bytes[idx + 1] = blend_c(color.1, bytes[idx + 1]);
    bytes[idx + 2] = blend_c(color.2, bytes[idx + 2]);
    bytes[idx + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

pub(crate) fn refresh_thumbnail_model(app: &mut AppState, window: &AppWindow) {
    let count = app.page_count() as usize;
    let mut items = Vec::with_capacity(count);
    for index in 0..app.page_count() {
        items.push(thumbnail_item(app, index));
    }
    window.set_thumbnail_items(ModelRc::new(VecModel::from(items)));
}

pub(crate) fn refresh_tool_thumbnails(
    window: &AppWindow,
    app: &mut AppState,
    selected_range: &str,
) {
    let selected = super::callbacks::selected_tool_pages(selected_range, app.page_count());
    let count = app.page_count() as usize;
    let mut items = Vec::with_capacity(count);
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
        items.push(ThumbnailItem {
            page_index: index as i32,
            page_number: thumbnail_page_label(app.preferences.language.resolve(), index),
            width: display_width,
            height: display_height,
            bitmap: image.clone().unwrap_or_default(),
            has_bitmap: image.is_some(),
            is_selected: selected.contains(&(index + 1)),
        });
    }
    window.set_thumbnail_items(ModelRc::new(VecModel::from(items)));
}

pub(crate) fn refresh_tab_model(app: &AppState, window: &AppWindow) {
    let active = app.application.tabs.active_id();
    let items = app
        .application
        .tabs
        .tabs()
        .iter()
        .map(|tab| TabItem {
            id: tab.id.to_slint_id(),
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

    #[test]
    fn compute_overlay_dimensions_matches_aspect_ratio_and_caps_budget() {
        let (w, h) = compute_overlay_dimensions(595.0, 842.0);
        assert_eq!((w, h), (1200, 1697));
        assert!((w * h * 4) as usize <= PAGE_ANNOTATION_BUDGET);

        let (w_let, h_let) = compute_overlay_dimensions(612.0, 792.0);
        assert_eq!((w_let, h_let), (1200, 1553));
        assert!((w_let * h_let * 4) as usize <= PAGE_ANNOTATION_BUDGET);

        let (w_huge, h_huge) = compute_overlay_dimensions(4000.0, 4000.0);
        assert!((w_huge * h_huge * 4) as usize <= PAGE_ANNOTATION_BUDGET);
        assert_eq!(w_huge, h_huge);
    }

    #[test]
    fn blend_pixel_safe_preserves_translucent_yellow_on_transparent_canvas() {
        let mut pixels = [0u8; 4];
        let yellow = (250, 204, 21, 96);
        blend_pixel_safe(&mut pixels, 1, 0, 0, yellow);
        assert_eq!(pixels, [250, 204, 21, 96]);

        let red = (255, 0, 0, 255);
        blend_pixel_safe(&mut pixels, 1, 0, 0, red);
        assert_eq!(pixels, [255, 0, 0, 255]);
    }

    #[test]
    fn draw_capsule_segment_aa_renders_anti_aliased_segment() {
        let mut pixels = [0u8; 20 * 20 * 4];
        draw_capsule_segment_aa(
            &mut pixels,
            20,
            20,
            (5.0, 10.0),
            (15.0, 10.0),
            2.0,
            (0, 0, 0, 255),
        );
        let center_idx = (10 * 20 + 10) * 4;
        assert_eq!(pixels[center_idx + 3], 255);
        assert_eq!(pixels[center_idx], 0);

        let outside_idx = 0;
        assert_eq!(pixels[outside_idx + 3], 0);
    }
}
