mod generated {
    slint::slint! {
        import {
            AppWindow,
            ThemeTokens,
            TabItem,
            SelectionBox,
            OverlayRectData,
            BookmarkItem,
            PageItem,
            ThumbnailItem,
            OutlineItem,
            RecentFileItem,
        } from "../ui/app_window.slint";

        export {
            AppWindow,
            ThemeTokens,
            TabItem,
            SelectionBox,
            OverlayRectData,
            BookmarkItem,
            PageItem,
            ThumbnailItem,
            OutlineItem,
            RecentFileItem,
        }
    }
}

pub use generated::*;

#[cfg(test)]
#[deny(clippy::all, clippy::pedantic)]
mod tests {
    use super::{
        AppWindow, BookmarkItem, OutlineItem, OverlayRectData, PageItem, RecentFileItem,
        SelectionBox, TabItem, ThumbnailItem,
    };
    use slint::{Color, Image, Model, ModelRc, SharedString, VecModel};
    use std::rc::Rc;

    #[test]
    fn exported_ui_structs_preserve_field_values_and_model_contracts() {
        let selection = SelectionBox {
            x: 12.0,
            y: 24.0,
            width: 120.0,
            height: 18.0,
        };
        assert!((selection.x - 12.0).abs() < f32::EPSILON);
        assert!((selection.width - 120.0).abs() < f32::EPSILON);

        let overlay = OverlayRectData {
            x_ratio: 0.1,
            y_ratio: 0.2,
            width_ratio: 0.5,
            height_ratio: 0.05,
            color: Color::from_argb_u8(128, 255, 235, 59),
        };
        assert!((overlay.width_ratio - 0.5).abs() < f32::EPSILON);
        assert_eq!(overlay.color.alpha(), 128);

        let bookmark = BookmarkItem {
            title: SharedString::from("Intro"),
            page_index: 0,
            page_number: SharedString::from("1"),
        };
        assert_eq!(bookmark.title.as_str(), "Intro");
        assert_eq!(bookmark.page_index, 0);

        let page = PageItem {
            page_index: 2,
            page_number: SharedString::from("3"),
            width: 612.0,
            height: 792.0,
            y_offset: 1600.0,
            bitmap: Image::default(),
            has_bitmap: false,
            selection_boxes: ModelRc::from(Rc::new(VecModel::from(vec![selection.clone()]))),
            search_highlights: ModelRc::default(),
        };
        assert_eq!(page.page_index, 2);
        assert_eq!(page.selection_boxes.row_count(), 1);
        assert_eq!(page.search_highlights.row_count(), 0);

        let thumb = ThumbnailItem {
            page_index: 1,
            page_number: SharedString::from("2"),
            width: 120.0,
            height: 160.0,
            bitmap: Image::default(),
            has_bitmap: false,
            is_selected: true,
        };
        assert!(thumb.is_selected);

        let outline = OutlineItem {
            title: SharedString::from("Chapter 1"),
            page_index: 4,
            depth: 1,
            has_children: true,
            expanded: false,
        };
        assert!(outline.has_children);
        assert!(!outline.expanded);

        let recent = RecentFileItem {
            name: SharedString::from("spec.pdf"),
            path: SharedString::from("C:/docs/spec.pdf"),
        };
        assert_eq!(recent.name.as_str(), "spec.pdf");

        let tab = TabItem {
            id: 7,
            title: SharedString::from("spec.pdf"),
            is_active: true,
            is_loading: false,
        };
        assert_eq!(tab.id, 7);
        assert!(tab.is_active);
        assert!(!tab.is_loading);
    }

    #[test]
    fn context_menu_exposes_only_single_canonical_action_callbacks() {
        type ActionCallbackSetter = fn(&AppWindow, Box<dyn Fn()>);
        let callbacks: [ActionCallbackSetter; 8] = [
            AppWindow::on_request_copy,
            AppWindow::on_context_find_selection,
            AppWindow::on_context_highlight_selection,
            AppWindow::on_request_select_all,
            AppWindow::on_rotate_view_cw,
            AppWindow::on_request_fit_page,
            AppWindow::on_request_prev_page,
            AppWindow::on_request_next_page,
        ];
        assert_eq!(callbacks.len(), 8);

        let context_menu_slint = include_str!("../ui/components/context_menu.slint");
        for canonical in [
            "root.request-copy();",
            "root.context-find-selection();",
            "root.context-highlight-selection();",
            "root.request-select-all();",
            "root.rotate-view-cw();",
            "root.request-fit-page();",
            "root.request-prev-page();",
            "root.request-next-page();",
        ] {
            assert_eq!(
                context_menu_slint.matches(canonical).count(),
                1,
                "expected context menu to invoke `{canonical}` exactly once"
            );
        }

        for duplicate in [
            "root.copy-selection()",
            "root.fit-page()",
            "root.select-all()",
            "root.prev-page()",
            "root.next-page()",
        ] {
            assert!(
                !context_menu_slint.contains(duplicate),
                "unexpected duplicate callback `{duplicate}` in context_menu.slint"
            );
        }
    }

    #[test]
    fn drawing_and_pan_callbacks_and_contracts_are_exposed() {
        type VoidCallbackSetter = fn(&AppWindow, Box<dyn Fn()>);
        type IntCallbackSetter = fn(&AppWindow, Box<dyn Fn(i32)>);
        type BoolGetter = fn(&AppWindow) -> bool;
        type IntGetter = fn(&AppWindow) -> i32;
        type FloatGetter = fn(&AppWindow) -> f32;

        let callbacks: [VoidCallbackSetter; 3] = [
            AppWindow::on_toggle_pan_mode,
            AppWindow::on_drawing_redo,
            AppWindow::on_toggle_drawing_toolbar_position,
        ];
        assert_eq!(callbacks.len(), 3);

        let int_callbacks: [IntCallbackSetter; 1] = [AppWindow::on_set_drawing_eraser_size];
        assert_eq!(int_callbacks.len(), 1);

        let bool_getters: [BoolGetter; 4] = [
            AppWindow::get_pan_mode_active,
            AppWindow::get_drawing_can_undo,
            AppWindow::get_drawing_can_redo,
            AppWindow::get_drawing_toolbar_at_bottom,
        ];
        assert_eq!(bool_getters.len(), 4);

        let int_getters: [IntGetter; 1] = [AppWindow::get_drawing_eraser_size_index];
        assert_eq!(int_getters.len(), 1);

        let float_getters: [FloatGetter; 1] = [AppWindow::get_drawing_eraser_diameter_norm];
        assert_eq!(float_getters.len(), 1);
    }
}
