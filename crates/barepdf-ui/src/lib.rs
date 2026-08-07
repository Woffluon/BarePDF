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

    // Modern Fluent 2 Glassmorphism Button
    component FluentButton inherits Rectangle {
        in property <string> text: "";
        in property <bool> enabled: true;
        in property <bool> active: false;
        in property <bool> primary: false;
        callback clicked();

        height: 32px;
        min-width: 36px;
        border-radius: 6px;

        background: !root.enabled ? #00000000 :
                    root.primary ? (touch.has-hover ? #0086f0 : (touch.pressed ? #005a9e : #0078d4)) :
                    root.active ? (touch.has-hover ? #0078d444 : #0078d428) :
                    (touch.has-hover ? #ffffff18 : (touch.pressed ? #ffffff08 : #ffffff0a));

        border-width: 1px;
        border-color: !root.enabled ? #00000000 :
                      root.primary ? (touch.has-hover ? #0094ff : #0078d4) :
                      root.active ? #0078d499 :
                      (touch.has-hover ? #ffffff30 : #ffffff14);

        drop-shadow-blur: root.primary && touch.has-hover ? 8px : 0px;
        drop-shadow-color: #0078d460;

        touch := TouchArea {
            enabled: root.enabled;
            clicked => { root.clicked(); }
        }

        HorizontalLayout {
            padding-left: 11px;
            padding-right: 11px;
            alignment: center;

            Text {
                text: root.text;
                font-size: 12px;
                font-weight: 500;
                color: !root.enabled ? #555555 : (root.primary ? #ffffff : (root.active ? #60a5fa : #f0f0f0));
                vertical-alignment: center;
                horizontal-alignment: center;
            }
        }
    }

    // Glassmorphic Fluent Context Menu
    component ContextMenu inherits Rectangle {
        in property <length> menu_x: 0px;
        in property <length> menu_y: 0px;
        in property <bool> has_selection: false;
        in property <string> text_copy: "Copy";
        in property <string> text_select_all: "Select All";
        callback copy_clicked();
        callback select_all_clicked();
        callback dismiss();

        background: #00000001;

        TouchArea {
            clicked => { dismiss(); }
        }

        Rectangle {
            x: Math.min(root.menu_x, parent.width - 175px);
            y: Math.min(root.menu_y, parent.height - 95px);
            width: 170px;
            height: root.has_selection ? 80px : 44px;
            background: #25272cdd;
            border-radius: 8px;
            border-width: 1px;
            border-color: #ffffff28;
            drop-shadow-blur: 16px;
            drop-shadow-color: #000000b0;

            VerticalLayout {
                padding: 5px;
                spacing: 3px;

                if (root.has_selection) : Rectangle {
                    height: 34px;
                    border-radius: 5px;
                    background: copy_touch.has-hover ? #0078d4 : #00000000;

                    copy_touch := TouchArea {
                        clicked => { copy_clicked(); }
                    }

                    HorizontalLayout {
                        padding-left: 12px;
                        padding-right: 12px;
                        alignment: space-between;

                        Text {
                            text: root.text_copy;
                            font-size: 12px;
                            font-weight: 500;
                            color: #ffffff;
                            vertical-alignment: center;
                        }
                        Text {
                            text: "Ctrl+C";
                            font-size: 11px;
                            color: #bbbbbb;
                            vertical-alignment: center;
                        }
                    }
                }

                Rectangle {
                    height: 34px;
                    border-radius: 5px;
                    background: sa_touch.has-hover ? #0078d4 : #00000000;

                    sa_touch := TouchArea {
                        clicked => { select_all_clicked(); }
                    }

                    HorizontalLayout {
                        padding-left: 12px;
                        padding-right: 12px;
                        alignment: space-between;

                        Text {
                            text: root.text_select_all;
                            font-size: 12px;
                            font-weight: 500;
                            color: #ffffff;
                            vertical-alignment: center;
                        }
                        Text {
                            text: "Ctrl+A";
                            font-size: 11px;
                            color: #bbbbbb;
                            vertical-alignment: center;
                        }
                    }
                }
            }
        }
    }

    // Glassmorphic Password Dialog Modal
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

        background: #000000bb;

        Rectangle {
            width: 440px;
            height: 240px;
            background: #24262bdd;
            border-radius: 12px;
            border-width: 1px;
            border-color: #ffffff28;
            drop-shadow-blur: 24px;
            drop-shadow-color: #000000d0;

            VerticalLayout {
                padding: 24px;
                spacing: 16px;

                Text {
                    text: root.text_title;
                    font-size: 19px;
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

    // Glassmorphic Settings Dialog Modal
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

        background: #000000bb;

        Rectangle {
            width: 480px;
            height: 330px;
            background: #24262bdd;
            border-radius: 12px;
            border-width: 1px;
            border-color: #ffffff28;
            drop-shadow-blur: 24px;
            drop-shadow-color: #000000d0;

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
                    Text { text: root.text_language; font-size: 13px; font-weight: 600; color: #bbbbbb; }
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
                    Text { text: root.text_theme; font-size: 13px; font-weight: 600; color: #bbbbbb; }
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
        preferred-width: 1200px;
        preferred-height: 860px;
        background: #0f1013;

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
        in-out property <bool> context_menu_open: false;
        in-out property <length> context_menu_x: 0px;
        in-out property <length> context_menu_y: 0px;
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
                    if (root.context_menu_open) {
                        root.context_menu_open = false;
                        return accept;
                    }
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
            background: #08090b;

            TouchArea {
                clicked => { root.request_next_page(); }
            }

            Rectangle {
                width: Math.min(parent.width - 40px, root.page_display_width);
                height: Math.min(parent.height - 40px, root.page_display_height);
                x: (parent.width - self.width) / 2;
                y: (parent.height - self.height) / 2;
                background: #ffffff;
                drop-shadow-blur: 24px;
                drop-shadow-color: #000000e0;

                Image {
                    source: root.page_bitmap;
                    width: 100%;
                    height: 100%;
                }
            }

            Rectangle {
                y: 16px;
                height: 32px;
                width: 260px;
                x: (parent.width - self.width) / 2;
                background: #1e2025dd;
                border-radius: 16px;
                border-width: 1px;
                border-color: #ffffff20;

                Text {
                    text: "Press Esc to exit presentation";
                    font-size: 12px;
                    color: #bbbbbb;
                    horizontal-alignment: center;
                    vertical-alignment: center;
                }
            }
        }

        // Normal View & Fullscreen View (window_mode != 2)
        if (root.window_mode != 2) : VerticalLayout {
            padding: 0px;
            spacing: 0px;

            // Top Command Bar (Fluent 2 Translucent Glass styled)
            if (root.window_mode != 1) : Rectangle {
                height: 48px;
                background: #1a1b1edd;
                border-width: 1px;
                border-color: #ffffff14;

                HorizontalLayout {
                    padding-left: 14px;
                    padding-right: 14px;
                    spacing: 10px;
                    alignment: space-between;

                    // Left group: Open, Sidebar, Navigation Pill
                    HorizontalLayout {
                        spacing: 8px;

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

                        Rectangle { width: 1px; height: 20px; background: #ffffff18; }

                        // Glass Pill for Page Navigation
                        Rectangle {
                            height: 32px;
                            background: #ffffff06;
                            border-radius: 6px;
                            border-width: 1px;
                            border-color: #ffffff12;

                            HorizontalLayout {
                                padding-left: 4px;
                                padding-right: 4px;
                                spacing: 4px;

                                FluentButton {
                                    text: "‹";
                                    enabled: root.has_document;
                                    clicked => { root.request_prev_page(); }
                                }

                                Rectangle {
                                    height: 26px;
                                    min-width: 76px;
                                    background: #00000030;
                                    border-radius: 4px;

                                    Text {
                                        text: root.current_page_str + " / " + root.total_pages_str;
                                        font-size: 12px;
                                        font-weight: 600;
                                        color: #e0e0e0;
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
                        }
                    }

                    // Center group: Zoom & View Mode Glass Pill
                    HorizontalLayout {
                        spacing: 8px;

                        Rectangle {
                            height: 32px;
                            background: #ffffff06;
                            border-radius: 6px;
                            border-width: 1px;
                            border-color: #ffffff12;

                            HorizontalLayout {
                                padding-left: 4px;
                                padding-right: 4px;
                                spacing: 4px;

                                FluentButton {
                                    text: "−";
                                    enabled: root.has_document;
                                    clicked => { root.request_zoom_out(); }
                                }

                                Rectangle {
                                    height: 26px;
                                    min-width: 58px;
                                    background: #00000030;
                                    border-radius: 4px;

                                    Text {
                                        text: root.zoom_str;
                                        font-size: 12px;
                                        font-weight: 600;
                                        color: #e0e0e0;
                                        horizontal-alignment: center;
                                        vertical-alignment: center;
                                    }
                                }

                                FluentButton {
                                    text: "+";
                                    enabled: root.has_document;
                                    clicked => { root.request_zoom_in(); }
                                }
                            }
                        }

                        Rectangle { width: 1px; height: 20px; background: #ffffff18; }

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

                    // Right group: Fullscreen, Presentation, Settings
                    HorizontalLayout {
                        spacing: 8px;

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

                // Segmented Fluent Acrylic Sidebar
                if (root.sidebar_visible && root.has_document) : Rectangle {
                    width: 240px;
                    background: #16171add;
                    border-width: 1px;
                    border-color: #ffffff12;

                    VerticalLayout {
                        padding: 10px;
                        spacing: 10px;

                        // Glass Segmented Tab Container
                        Rectangle {
                            height: 36px;
                            background: #0f1013;
                            border-radius: 6px;
                            border-width: 1px;
                            border-color: #ffffff10;

                            HorizontalLayout {
                                padding: 3px;
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
                        }

                        // Sidebar Thumbnail List with Floating Cards
                        if root.sidebar_tab == 0 : ScrollView {
                            VerticalLayout {
                                padding: 4px;
                                spacing: 12px;

                                for thumb in root.thumbnail_items : Rectangle {
                                    height: thumb.height + 28px;
                                    background: thumb.is_selected ? #0078d430 : (thumb_touch.has-hover ? #ffffff10 : #ffffff06);
                                    border-radius: 8px;
                                    border-width: thumb.is_selected ? 2px : 1px;
                                    border-color: thumb.is_selected ? #0078d4 : #ffffff14;
                                    drop-shadow-blur: thumb.is_selected ? 10px : 0px;
                                    drop-shadow-color: #0078d450;

                                    thumb_touch := TouchArea {
                                        clicked => { root.request_select_page(thumb.page_index); }
                                    }

                                    VerticalLayout {
                                        padding: 6px;
                                        alignment: center;
                                        spacing: 6px;

                                        Rectangle {
                                            width: thumb.width;
                                            height: thumb.height;
                                            background: #ffffff;
                                            border-radius: 2px;

                                            Image {
                                                source: thumb.bitmap;
                                                width: 100%;
                                                height: 100%;
                                            }
                                        }

                                        Text {
                                            text: thumb.page_number;
                                            font-size: 11px;
                                            font-weight: 500;
                                            color: thumb.is_selected ? #ffffff : #aaaaaa;
                                            horizontal-alignment: center;
                                        }
                                    }
                                }
                            }
                        }

                        // Outline View
                        if root.sidebar_tab == 1 : Rectangle {
                            background: #0f1013;
                            border-radius: 6px;

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

                // Document Viewport Workspace (Floating Paper Cards on Deep Dark Slate)
                Rectangle {
                    background: #0f1013;

                    // Empty State Screen
                    if (!root.has_document && !root.password_required) : VerticalLayout {
                        alignment: center;
                        spacing: 18px;

                        Text {
                            text: "BarePDF";
                            font-size: 40px;
                            font-weight: 800;
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
                        viewport-width: Math.max(self.width, root.page_display_width + 60px);
                        viewport-height: Math.max(self.height, root.page_display_height + 60px);

                        page_container := Rectangle {
                            width: root.page_display_width;
                            height: root.page_display_height;
                            x: Math.max(30px, (parent.viewport-width - self.width) / 2);
                            y: Math.max(30px, (parent.viewport-height - self.height) / 2);
                            background: #ffffff;
                            border-radius: 4px;
                            drop-shadow-blur: 24px;
                            drop-shadow-offset-y: 8px;
                            drop-shadow-color: #000000c0;

                            Image {
                                source: root.page_bitmap;
                                width: 100%;
                                height: 100%;
                            }

                            for box in root.visible_pages[0].selection_boxes : Rectangle {
                                x: box.x;
                                y: box.y;
                                width: box.width;
                                height: box.height;
                                background: #0078d44d;
                                border-width: 0px;
                            }

                            // Single Page Mouse TouchArea for text selection & right click
                            TouchArea {
                                pointer-event(evt) => {
                                    if (evt.kind == PointerEventKind.down) {
                                        if (evt.button == PointerEventButton.right) {
                                            root.context_menu_x = self.mouse-x;
                                            root.context_menu_y = self.mouse-y;
                                            root.context_menu_open = true;
                                        } else {
                                            root.context_menu_open = false;
                                            root.pointer_down(0, self.mouse-x, self.mouse-y, 1);
                                        }
                                    }
                                    if (evt.kind == PointerEventKind.up && evt.button == PointerEventButton.left) {
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
                        viewport-width: Math.max(self.width, root.page_display_width + 60px);
                        viewport-height: Math.max(self.height, root.document_total_height + 60px);
                        viewport-y <=> root.current_scroll_y;

                        for page in root.visible_pages : Rectangle {
                            width: page.width;
                            height: page.height;
                            x: Math.max(30px, (parent.viewport-width - page.width) / 2);
                            y: page.y_offset;
                            background: #ffffff;
                            border-radius: 4px;
                            drop-shadow-blur: 24px;
                            drop-shadow-offset-y: 8px;
                            drop-shadow-color: #000000c0;

                            if (page.has_bitmap) : Image {
                                source: page.bitmap;
                                width: 100%;
                                height: 100%;
                            }

                            // Render text selection highlight boxes over page (Chrome PDF Viewer style)
                            for box in page.selection_boxes : Rectangle {
                                x: box.x;
                                y: box.y;
                                width: box.width;
                                height: box.height;
                                background: #0078d44d;
                                border-width: 0px;
                            }

                            // Page Mouse Interaction TouchArea for left-drag selection & right click context menu
                            TouchArea {
                                pointer-event(evt) => {
                                    if (evt.kind == PointerEventKind.down) {
                                        if (evt.button == PointerEventButton.right) {
                                            root.context_menu_x = self.mouse-x;
                                            root.context_menu_y = self.mouse-y + page.y_offset;
                                            root.context_menu_open = true;
                                        } else {
                                            root.context_menu_open = false;
                                            root.pointer_down(page.page_index, self.mouse-x, self.mouse-y, 1);
                                        }
                                    }
                                    if (evt.kind == PointerEventKind.up && evt.button == PointerEventButton.left) {
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

                    if (root.context_menu_open) : ContextMenu {
                        menu_x: root.context_menu_x;
                        menu_y: root.context_menu_y;
                        has_selection: root.has_selection;
                        text_copy: root.text_copy;
                        text_select_all: root.text_select_all;
                        copy_clicked() => {
                            root.context_menu_open = false;
                            root.request_copy();
                        }
                        select_all_clicked() => {
                            root.context_menu_open = false;
                            root.request_select_all();
                        }
                        dismiss() => {
                            root.context_menu_open = false;
                        }
                    }
                }
            }

            // Clean Fluent Status Bar
            if (root.window_mode != 1) : Rectangle {
                height: 26px;
                background: #16171add;
                border-width: 1px;
                border-color: #ffffff12;

                HorizontalLayout {
                    padding-left: 14px;
                    padding-right: 14px;
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
