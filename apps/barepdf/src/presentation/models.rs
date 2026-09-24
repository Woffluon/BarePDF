#![allow(clippy::cast_possible_wrap, clippy::cast_precision_loss)]

use super::state::AppState;
use super::ui::{
    compute_search_highlights, compute_selection_boxes, ensure_layout, visible_page_indices,
};
use barepdf_i18n::{t, ResolvedLanguage};
use barepdf_render::RenderKind;
use barepdf_ui::{AppWindow, PageItem, TabItem, ThumbnailItem};
use slint::{Model, ModelRc, SharedString, VecModel};

pub(crate) fn refresh_page_model(app: &mut AppState, window: &AppWindow) {
    ensure_layout(app);
    let indices = visible_page_indices(app, window);
    app.visible_page_indices.clone_from(&indices);

    let model = VecModel::default();
    for index in indices {
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

    if let Some(page) = app.layout.pages.get(app.current_page as usize) {
        window.set_page_display_width(page.width as f32);
        window.set_page_display_height(page.height as f32);
        let page_width = app
            .page_dimensions
            .get(app.current_page as usize)
            .map_or(app.first_page_dimensions.0, |dimensions| dimensions.0)
            .max(1.0);
        let effective_zoom = page.width as f32 / page_width;
        window.set_zoom_str(SharedString::from(format!(
            "{}%",
            (effective_zoom * 100.0).round()
        )));
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
