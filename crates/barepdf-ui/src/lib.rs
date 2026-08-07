use slint::ComponentHandle;

slint::slint! {
    import { Button, LineEdit, ScrollView } from "std-widgets.slint";

    export struct SelectionBox {
        x: length,
        y: length,
        width: length,
        height: length,
    }

    export struct PageItem {
        page_index: int,
        page_number: string,
        width: length,
        height: length,
        y_offset: length,
        bitmap: image,
        has_bitmap: bool,
        selection_boxes: [SelectionBox],
    }

    export struct ThumbnailItem {
        page_index: int,
        page_number: string,
        width: length,
        height: length,
        bitmap: image,
        is_selected: bool,
    }

    component FluentButton inherits Rectangle {
        in property <string> text: "";
        in property <bool> enabled: true;
        in property <bool> active: false;
        in property <bool> primary: false;
        callback clicked();

        height: 30px;
        min-width: 36px;
        border-radius: 4px;
        background: !root.enabled ? #00000000 :
                    root.primary ? (touch.has-hover ? #0066b8 : #0078d4) :
                    root.active ? #0078d433 :
                    (touch.has-hover ? #ffffff12 : #ffffff08);
        border-width: 1px;
        border-color: root.primary ? #0078d4 :
                      root.active ? #0078d480 :
                      (touch.has-hover ? #ffffff25 : #ffffff12);

        touch := TouchArea {
            enabled: root.enabled;
            clicked => { root.clicked(); }
        }

        HorizontalLayout {
            padding-left: 10px;
            padding-right: 10px;
            alignment: center;

            Text {
                text: root.text;
                font-size: 12px;
                font-weight: 500;
                color: !root.enabled ? #666666 : (root.primary ? #ffffff : (root.active ? #60a5fa : #e0e0e0));
                vertical-alignment: center;
                horizontal-alignment: center;
            }
        }
    }

    component PasswordModal inherits Rectangle {
        in property <string> file_name: "";
        in-out property <string> password_input: "";
        in property <string> text_title: "Password Required";
        in property <string> text_desc: "This document is encrypted. Enter password to open:";
        in property <string> text_placeholder: "Password";
        in property <string> text_unlock: "Unlock";
        in property <string> text_cancel: "Cancel";
        callback submit_password(string);
        callback cancel();

        background: #000000aa;

        Rectangle {
            width: 420px;
            height: 230px;
            background: #1e1e1e;
            border-radius: 8px;
            border-width: 1px;
            border-color: #ffffff1f;

            VerticalLayout {
                padding: 24px;
                spacing: 16px;

                Text {
                    text: root.text_title;
                    font-size: 18px;
                    font-weight: 700;
                    color: #ffffff;
                }

                Text {
                    text: root.text_desc;
                    font-size: 13px;
                    color: #cccccc;
                    wrap: word-wrap;
                }

                LineEdit {
                    text <=> password_input;
                    placeholder-text: root.text_placeholder;
                    accepted => { submit_password(password_input); }
                }

                HorizontalLayout {
                    spacing: 12px;
                    alignment: end;

                    FluentButton {
                        text: root.text_cancel;
                        clicked => { cancel(); }
                    }
                    FluentButton {
                        text: root.text_unlock;
                        primary: true;
                        clicked => { submit_password(password_input); }
                    }
                }
            }
        }
    }

    component SettingsModal inherits Rectangle {
        in property <int> current_language: 0; // 0: System, 1: English, 2: Turkish
        in property <int> current_theme: 0;    // 0: System, 1: Light, 2: Dark
        in property <int> current_view_mode: 0;// 0: Continuous, 1: Single
        in property <string> text_title: "Preferences";
        in property <string> text_language: "Language";
        in property <string> text_theme: "Theme";
        in property <string> text_view_mode: "Default View Mode";
        in property <string> text_close: "Close";
        callback select_language(int);
        callback select_theme(int);
        callback select_view_mode(int);
        callback close();

        background: #000000aa;

        Rectangle {
            width: 460px;
            height: 320px;
            background: #1e1e1e;
            border-radius: 8px;
            border-width: 1px;
            border-color: #ffffff1f;

            VerticalLayout {
                padding: 24px;
                spacing: 20px;

                Text {
                    text: root.text_title;
                    font-size: 20px;
                    font-weight: 700;
                    color: #ffffff;
                }

                // Language selection
                VerticalLayout {
                    spacing: 8px;
                    Text { text: root.text_language; font-size: 13px; color: #aaaaaa; }
                    HorizontalLayout {
                        spacing: 8px;
                        FluentButton {
                            text: "System Default";
                            active: root.current_language == 0;
                            clicked => { select_language(0); }
                        }
                        FluentButton {
                            text: "English";
                            active: root.current_language == 1;
                            clicked => { select_language(1); }
                        }
                        FluentButton {
                            text: "Türkçe";
                            active: root.current_language == 2;
                            clicked => { select_language(2); }
                        }
                    }
                }

                // Theme selection
                VerticalLayout {
                    spacing: 8px;
                    Text { text: root.text_theme; font-size: 13px; color: #aaaaaa; }
                    HorizontalLayout {
                        spacing: 8px;
                        FluentButton {
                            text: "System";
                            active: root.current_theme == 0;
                            clicked => { select_theme(0); }
                        }
                        FluentButton {
                            text: "Light";
                            active: root.current_theme == 1;
                            clicked => { select_theme(1); }
                        }
                        FluentButton {
                            text: "Dark";
                            active: root.current_theme == 2;
                            clicked => { select_theme(2); }
                        }
                    }
                }

                HorizontalLayout {
                    alignment: end;
                    FluentButton {
                        text: root.text_close;
                        primary: true;
                        clicked => { close(); }
                    }
                }
            }
        }
    }

    export component AppWindow inherits Window {
        title: root.document_title != "" ? root.document_title + " — BarePDF" : "BarePDF";
        icon: @image-url("../../../assets/logo.svg");
        preferred-width: 1180px;
        preferred-height: 840px;
        background: #141414;

        in property <string> document_title: "";
        in property <string> status_text: "Ready";
        in property <string> current_page_str: "1";
        in property <string> total_pages_str: "0";
        in property <string> zoom_str: "100%";
        in property <image> page_bitmap;
        in property <length> page_display_width: 800px;
        in property <length> page_display_height: 1040px;
        in property <length> document_total_height: 1040px;
        in property <[PageItem]> visible_pages: [];
        in property <[ThumbnailItem]> thumbnail_items: [];
        in-out property <length> current_scroll_y: 0px;
        in property <bool> has_document: false;
        in property <bool> has_selection: false;
        in-out property <bool> password_required: false;
        in-out property <bool> settings_open: false;
        in property <string> protected_file_name: "";
        in-out property <bool> sidebar_visible: true;
        in-out property <int> sidebar_tab: 0; // 0: Thumbnails, 1: Outline
        in-out property <int> window_mode: 0; // 0: Normal, 1: FullScreen, 2: Presentation
        in property <string> view_mode_label: "Continuous";
        in property <int> current_language: 0;
        in property <int> current_theme: 0;

        // Localized string bindings
        in property <string> text_open: "Open PDF";
        in property <string> text_sidebar: "Sidebar";
        in property <string> text_thumbnails: "Thumbnails";
        in property <string> text_outline: "Outline";
        in property <string> text_view: "View";
        in property <string> text_zoom_in: "Zoom In";
        in property <string> text_zoom_out: "Zoom Out";
        in property <string> text_fit_width: "Fit Width";
        in property <string> text_fit_page: "Fit Page";
        in property <string> text_actual_size: "Actual Size";
        in property <string> text_fullscreen: "Full Screen";
        in property <string> text_presentation: "Presentation";
        in property <string> text_settings: "Settings";
        in property <string> text_copy: "Copy";
        in property <string> text_select_all: "Select All";
        in property <string> text_close: "Close";
        in property <string> text_empty_title: "No Document Loaded";
        in property <string> text_empty_desc: "Click 'Open PDF' or drag a PDF file here to begin reading.";

        callback request_open_file();
        callback request_next_page();
        callback request_prev_page();
        callback request_first_page();
        callback request_last_page();
        callback request_zoom_in();
        callback request_zoom_out();
        callback request_fit_width();
        callback request_fit_page();
        callback request_actual_size();
        callback request_toggle_sidebar();
        callback request_toggle_fullscreen();
        callback request_presentation_mode();
        callback request_exit_special_mode();
        callback request_unlock_password(string);
        callback request_select_page(int);
        callback request_toggle_view_mode();
        callback request_change_language(int);
        callback request_change_theme(int);
        callback request_copy();
        callback request_select_all();

        // Mouse pointer events for PDF text selection
        callback pointer_down(int, length, length, int);
        callback pointer_move(int, length, length);
        callback pointer_up(int, length, length);

        // Keyboard navigation & shortcuts
        FocusScope {
            key-pressed(event) => {
                if (event.text == "\u{001b}") { // Esc
                    if (root.settings_open) {
                        root.settings_open = false;
                        return accept;
                    }
                    root.request_exit_special_mode();
                    return accept;
                }
                if (event.text == "\u{f11}" || event.text == "F11") {
                    root.request_toggle_fullscreen();
                    return accept;
                }
                if (event.text == "\u{f5}" || event.text == "F5") {
                    root.request_presentation_mode();
                    return accept;
                }
                if (event.modifiers.control && (event.text == "c" || event.text == "C")) {
                    root.request_copy();
                    return accept;
                }
                if (event.modifiers.control && (event.text == "a" || event.text == "A")) {
                    root.request_select_all();
                    return accept;
                }
                if (event.modifiers.control && (event.text == "o" || event.text == "O")) {
                    root.request_open_file();
                    return accept;
                }
                return reject;
            }
        }

        // Presentation View (window_mode == 2)
        if (root.window_mode == 2) : Rectangle {
            background: #0a0a0a;

            TouchArea {
                clicked => { root.request_next_page(); }
            }

            Rectangle {
                width: Math.min(parent.width - 40px, root.page_display_width);
                height: Math.min(parent.height - 40px, root.page_display_height);
                x: (parent.width - self.width) / 2;
                y: (parent.height - self.height) / 2;
                background: #ffffff;

                Image {
                    source: root.page_bitmap;
                    width: 100%;
                    height: 100%;
                }
            }

            Rectangle {
                y: 12px;
                height: 28px;
                width: 240px;
                x: (parent.width - self.width) / 2;
                background: #000000cc;
                border-radius: 14px;

                Text {
                    text: "Press Esc to exit presentation";
                    font-size: 11px;
                    color: #aaaaaa;
                    horizontal-alignment: center;
                    vertical-alignment: center;
                }
            }
        }

        // Normal View & Fullscreen View (window_mode != 2)
        if (root.window_mode != 2) : VerticalLayout {
            padding: 0px;
            spacing: 0px;

            // Top Command Bar (Fluent styled)
            if (root.window_mode != 1) : Rectangle {
                height: 42px;
                background: #1c1c1c;
                border-width: 1px;
                border-color: #ffffff10;

                HorizontalLayout {
                    padding-left: 12px;
                    padding-right: 12px;
                    spacing: 8px;
                    alignment: space-between;

                    // Left group: Open, Sidebar, Navigation
                    HorizontalLayout {
                        spacing: 6px;

                        FluentButton {
                            text: root.text_open;
                            primary: true;
                            clicked => { root.request_open_file(); }
                        }

                        FluentButton {
                            text: root.text_sidebar;
                            active: root.sidebar_visible;
                            enabled: root.has_document;
                            clicked => { root.request_toggle_sidebar(); }
                        }

                        Rectangle { width: 1px; height: 18px; background: #ffffff18; }

                        FluentButton {
                            text: "‹";
                            enabled: root.has_document;
                            clicked => { root.request_prev_page(); }
                        }

                        Rectangle {
                            height: 30px;
                            min-width: 76px;
                            background: #141414;
                            border-radius: 4px;
                            border-width: 1px;
                            border-color: #ffffff12;

                            Text {
                                text: root.current_page_str + " / " + root.total_pages_str;
                                font-size: 12px;
                                color: #d0d0d0;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                        }

                        FluentButton {
                            text: "›";
                            enabled: root.has_document;
                            clicked => { root.request_next_page(); }
                        }
                    }

                    // Center group: Zoom & View
                    HorizontalLayout {
                        spacing: 6px;

                        FluentButton {
                            text: "−";
                            enabled: root.has_document;
                            clicked => { root.request_zoom_out(); }
                        }

                        Rectangle {
                            height: 30px;
                            min-width: 58px;
                            background: #141414;
                            border-radius: 4px;
                            border-width: 1px;
                            border-color: #ffffff12;

                            Text {
                                text: root.zoom_str;
                                font-size: 12px;
                                color: #d0d0d0;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                        }

                        FluentButton {
                            text: "+";
                            enabled: root.has_document;
                            clicked => { root.request_zoom_in(); }
                        }

                        Rectangle { width: 1px; height: 18px; background: #ffffff18; }

                        FluentButton {
                            text: root.view_mode_label;
                            enabled: root.has_document;
                            clicked => { root.request_toggle_view_mode(); }
                        }

                        FluentButton {
                            text: root.text_fit_width;
                            enabled: root.has_document;
                            clicked => { root.request_fit_width(); }
                        }

                        FluentButton {
                            text: root.text_fit_page;
                            enabled: root.has_document;
                            clicked => { root.request_fit_page(); }
                        }
                    }

                    // Right group: Copy, Fullscreen, Presentation, Settings
                    HorizontalLayout {
                        spacing: 6px;

                        FluentButton {
                            text: root.text_copy;
                            enabled: root.has_selection;
                            primary: root.has_selection;
                            clicked => { root.request_copy(); }
                        }

                        FluentButton {
                            text: root.text_fullscreen;
                            enabled: root.has_document;
                            clicked => { root.request_toggle_fullscreen(); }
                        }

                        FluentButton {
                            text: root.text_presentation;
                            enabled: root.has_document;
                            clicked => { root.request_presentation_mode(); }
                        }

                        FluentButton {
                            text: "⚙";
                            clicked => { root.settings_open = !root.settings_open; }
                        }
                    }
                }
            }

            // Main Workspace (Sidebar + PDF Viewport)
            HorizontalLayout {
                spacing: 0px;

                // Segmented Fluent Sidebar
                if (root.sidebar_visible && root.has_document) : Rectangle {
                    width: 230px;
                    background: #181818;
                    border-width: 1px;
                    border-color: #ffffff10;

                    VerticalLayout {
                        padding: 8px;
                        spacing: 8px;

                        HorizontalLayout {
                            spacing: 4px;

                            FluentButton {
                                text: root.text_thumbnails;
                                active: root.sidebar_tab == 0;
                                clicked => { root.sidebar_tab = 0; }
                            }
                            FluentButton {
                                text: root.text_outline;
                                active: root.sidebar_tab == 1;
                                clicked => { root.sidebar_tab = 1; }
                            }
                        }

                        // Sidebar Thumbnail List
                        if root.sidebar_tab == 0 : ScrollView {
                            VerticalLayout {
                                padding: 4px;
                                spacing: 10px;

                                for thumb in root.thumbnail_items : Rectangle {
                                    height: thumb.height + 26px;
                                    background: thumb.is_selected ? #0078d425 : (thumb_touch.has-hover ? #ffffff0d : #ffffff05);
                                    border-radius: 4px;
                                    border-width: thumb.is_selected ? 2px : 1px;
                                    border-color: thumb.is_selected ? #0078d4 : #ffffff10;

                                    thumb_touch := TouchArea {
                                        clicked => { root.request_select_page(thumb.page_index); }
                                    }

                                    VerticalLayout {
                                        padding: 4px;
                                        alignment: center;
                                        spacing: 4px;

                                        Rectangle {
                                            width: thumb.width;
                                            height: thumb.height;
                                            background: #ffffff;

                                            Image {
                                                source: thumb.bitmap;
                                                width: 100%;
                                                height: 100%;
                                            }
                                        }

                                        Text {
                                            text: thumb.page_number;
                                            font-size: 11px;
                                            color: thumb.is_selected ? #ffffff : #aaaaaa;
                                            horizontal-alignment: center;
                                        }
                                    }
                                }
                            }
                        }

                        // Outline View
                        if root.sidebar_tab == 1 : Rectangle {
                            background: #141414;
                            border-radius: 4px;

                            Text {
                                text: "No Outline available";
                                font-size: 12px;
                                color: #666666;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                        }
                    }
                }

                // Document Viewport Workspace
                Rectangle {
                    background: #101010;

                    // Empty State Screen
                    if (!root.has_document && !root.password_required) : VerticalLayout {
                        alignment: center;
                        spacing: 16px;

                        Text {
                            text: "BarePDF";
                            font-size: 36px;
                            font-weight: 700;
                            color: #ffffff;
                            horizontal-alignment: center;
                        }

                        Text {
                            text: root.text_empty_desc;
                            font-size: 14px;
                            color: #888888;
                            horizontal-alignment: center;
                        }

                        HorizontalLayout {
                            alignment: center;
                            FluentButton {
                                text: root.text_open;
                                primary: true;
                                clicked => { root.request_open_file(); }
                            }
                        }
                    }

                    // Single Page Viewport
                    if (root.has_document && root.view_mode_label == "Single Page") : ScrollView {
                        viewport-width: Math.max(self.width, root.page_display_width + 40px);
                        viewport-height: Math.max(self.height, root.page_display_height + 40px);

                        page_container := Rectangle {
                            width: root.page_display_width;
                            height: root.page_display_height;
                            x: Math.max(20px, (parent.viewport-width - self.width) / 2);
                            y: Math.max(20px, (parent.viewport-height - self.height) / 2);
                            background: #ffffff;
                            border-width: 1px;
                            border-color: #00000040;

                            Image {
                                source: root.page_bitmap;
                                width: 100%;
                                height: 100%;
                            }

                            // Single Page Mouse TouchArea for text selection
                            TouchArea {
                                pointer-event(evt) => {
                                    if (evt.kind == PointerEventKind.down) {
                                        root.pointer_down(0, self.mouse-x, self.mouse-y, 1);
                                    }
                                    if (evt.kind == PointerEventKind.up) {
                                        root.pointer_up(0, self.mouse-x, self.mouse-y);
                                    }
                                    if (evt.kind == PointerEventKind.move && self.pressed) {
                                        root.pointer_move(0, self.mouse-x, self.mouse-y);
                                    }
                                }
                            }
                        }
                    }

                    // Continuous Vertical Viewport
                    if (root.has_document && root.view_mode_label != "Single Page") : ScrollView {
                        viewport-width: Math.max(self.width, root.page_display_width + 40px);
                        viewport-height: Math.max(self.height, root.document_total_height + 40px);
                        viewport-y <=> root.current_scroll_y;

                        for page in root.visible_pages : Rectangle {
                            width: page.width;
                            height: page.height;
                            x: Math.max(20px, (parent.viewport-width - page.width) / 2);
                            y: page.y_offset;
                            background: #ffffff;
                            border-width: 1px;
                            border-color: #00000040;

                            if (page.has_bitmap) : Image {
                                source: page.bitmap;
                                width: 100%;
                                height: 100%;
                            }

                            // Render text selection highlight boxes over page
                            for box in page.selection_boxes : Rectangle {
                                x: box.x;
                                y: box.y;
                                width: box.width;
                                height: box.height;
                                background: #0078d440;
                                border-width: 1px;
                                border-color: #0078d480;
                            }

                            // Page Mouse Interaction TouchArea
                            TouchArea {
                                pointer-event(evt) => {
                                    if (evt.kind == PointerEventKind.down) {
                                        root.pointer_down(page.page_index, self.mouse-x, self.mouse-y, 1);
                                    }
                                    if (evt.kind == PointerEventKind.up) {
                                        root.pointer_up(page.page_index, self.mouse-x, self.mouse-y);
                                    }
                                    if (evt.kind == PointerEventKind.move && self.pressed) {
                                        root.pointer_move(page.page_index, self.mouse-x, self.mouse-y);
                                    }
                                }
                            }

                            if (!page.has_bitmap) : VerticalLayout {
                                alignment: center;
                                Text {
                                    text: "Loading " + page.page_number + "...";
                                    font-size: 13px;
                                    color: #888888;
                                    horizontal-alignment: center;
                                }
                            }
                        }
                    }

                    if (root.password_required) : PasswordModal {
                        file_name: root.protected_file_name;
                        submit_password(pwd) => { root.request_unlock_password(pwd); }
                        cancel => { root.password_required = false; }
                    }

                    if (root.settings_open) : SettingsModal {
                        current_language: root.current_language;
                        current_theme: root.current_theme;
                        text_title: root.text_settings;
                        text_close: root.text_close;
                        select_language(idx) => { root.request_change_language(idx); }
                        select_theme(idx) => { root.request_change_theme(idx); }
                        close => { root.settings_open = false; }
                    }
                }
            }

            // Clean Fluent Status Bar
            if (root.window_mode != 1) : Rectangle {
                height: 24px;
                background: #181818;
                border-width: 1px;
                border-color: #ffffff10;

                HorizontalLayout {
                    padding-left: 12px;
                    padding-right: 12px;
                    alignment: space-between;

                    Text {
                        text: root.status_text;
                        font-size: 11px;
                        color: #aaaaaa;
                        vertical-alignment: center;
                    }

                    Text {
                        text: root.has_document ? root.view_mode_label : "";
                        font-size: 11px;
                        color: #777777;
                        vertical-alignment: center;
                    }
                }
            }
        }
    }
}

pub struct UiApp {
    window: AppWindow,
}

impl UiApp {
    pub fn new() -> Result<Self, slint::PlatformError> {
        let window = AppWindow::new()?;
        Ok(Self { window })
    }

    pub fn run(&self) -> Result<(), slint::PlatformError> {
        self.window.run()
    }
}
