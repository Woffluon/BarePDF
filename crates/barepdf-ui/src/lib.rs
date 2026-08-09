use slint::ComponentHandle;

slint::slint! {
    import { LineEdit, ListView, Palette, ScrollView } from "std-widgets.slint";

    export global ThemeTokens {
        in-out property <int> theme-mode: 0;
        changed theme-mode => {
            Palette.color-scheme = theme-mode == 1 ? ColorScheme.light
                : (theme-mode == 2 ? ColorScheme.dark : ColorScheme.unknown);
        }
        out property <bool> dark: Palette.color-scheme == ColorScheme.dark;
        out property <color> window: dark ? #0f1114 : #f3f4f6;
        out property <color> command: dark ? #181a1e : #fbfbfc;
        out property <color> panel: dark ? #15171a : #ffffff;
        out property <color> canvas: dark ? #0f1114 : #e9ebee;
        out property <color> control: dark ? #22252a : #ffffff;
        out property <color> control-hover: dark ? #2b2e34 : #f0f1f3;
        out property <color> text: dark ? #f4f4f5 : #1f2328;
        out property <color> text-muted: dark ? #a7abb3 : #626972;
        out property <color> border: dark ? #ffffff18 : #1f23281f;
        out property <color> accent: #f69423;
        out property <color> accent-content: #241407;
        out property <color> selection: #f694232e;
        out property <color> danger: dark ? #ffb4ab : #b42318;
    }

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
        has_bitmap: bool,
        is_selected: bool,
    }

    export struct OutlineItem {
        title: string,
        page_index: int,
        depth: int,
        has_children: bool,
        expanded: bool,
    }

    export struct RecentFileItem {
        name: string,
        path: string,
    }

    component IconButton inherits Rectangle {
        in property <image> icon;
        in property <string> label: "";
        in property <string> tooltip: label;
        in property <bool> show-label: false;
        in property <bool> enabled: true;
        in property <bool> active: false;
        in property <bool> primary: false;
        callback clicked();

        height: 34px;
        min-width: show-label ? 42px : 34px;
        border-radius: 6px;
        background: !enabled ? #00000000
            : primary ? (touch.pressed ? #df7d13 : (touch.has-hover ? #ffa13a : ThemeTokens.accent))
            : active ? ThemeTokens.selection
            : (touch.pressed ? ThemeTokens.control-hover : (touch.has-hover ? ThemeTokens.control-hover : #00000000));
        border-width: enabled && (primary || active || touch.has-hover) ? 1px : 0px;
        border-color: primary ? #d87912 : (active ? ThemeTokens.accent : ThemeTokens.border);
        accessible-role: button;
        accessible-label: tooltip;

        touch := TouchArea {
            enabled: root.enabled;
            clicked => { root.clicked(); }
        }

        HorizontalLayout {
            height: root.height;
            padding-left: root.show-label ? 10px : 7px;
            padding-right: root.show-label ? 11px : 7px;
            spacing: root.show-label ? 7px : 0px;
            alignment: center;

            Rectangle {
                width: 20px;
                height: parent.height;
                Image {
                    source: root.icon;
                    width: 20px;
                    height: 20px;
                    y: (parent.height - self.height) / 2;
                    colorize: !root.enabled ? ThemeTokens.text-muted.with-alpha(0.45)
                        : root.primary ? ThemeTokens.accent-content : ThemeTokens.text;
                    image-fit: contain;
                    accessible-role: none;
                }
            }
            if root.show-label : Text {
                height: parent.height;
                text: root.label;
                color: root.primary ? ThemeTokens.accent-content : ThemeTokens.text;
                font-size: 12px;
                font-weight: 600;
                vertical-alignment: center;
                overflow: elide;
            }
        }
    }

    component TextButton inherits Rectangle {
        in property <string> text;
        in property <bool> active: false;
        callback clicked();
        height: 32px;
        min-width: 52px;
        border-radius: 6px;
        background: active ? ThemeTokens.selection : (touch.has-hover ? ThemeTokens.control-hover : #00000000);
        border-width: active ? 1px : 0px;
        border-color: ThemeTokens.accent;
        accessible-role: button;
        accessible-label: text;
        touch := TouchArea { clicked => { root.clicked(); } }
        Text {
            text: root.text;
            color: ThemeTokens.text;
            font-size: 12px;
            font-weight: active ? 600 : 500;
            horizontal-alignment: center;
            vertical-alignment: center;
        }
    }

    component PasswordPopover inherits Rectangle {
        in-out property <string> password-input: "";
        in property <string> file-name: "";
        in property <string> error-text: "";
        callback submit(string);
        callback cancel();
        background: #0000008a;
        TouchArea { clicked => { } }
        Rectangle {
            width: 420px;
            height: error-text == "" ? 230px : 258px;
            background: ThemeTokens.panel;
            border-radius: 12px;
            border-width: 1px;
            border-color: ThemeTokens.border;
            drop-shadow-blur: 24px;
            drop-shadow-color: #00000066;
            VerticalLayout {
                padding: 24px;
                spacing: 13px;
                Text { text: "Password required"; color: ThemeTokens.text; font-size: 19px; font-weight: 700; }
                Text { text: root.file-name; color: ThemeTokens.text-muted; font-size: 12px; overflow: elide; }
                if root.error-text != "" : Text { text: root.error-text; color: ThemeTokens.danger; font-size: 12px; }
                LineEdit {
                    text <=> root.password-input;
                    placeholder-text: "Password";
                    input-type: password;
                    accepted => { root.submit(root.password-input); }
                }
                HorizontalLayout {
                    spacing: 8px;
                    alignment: end;
                    TextButton { text: "Cancel"; clicked => { root.cancel(); } }
                    Rectangle {
                        width: 76px; height: 32px; border-radius: 6px; background: ThemeTokens.accent;
                        TouchArea { clicked => { root.submit(root.password-input); } }
                        Text { text: "Unlock"; color: ThemeTokens.accent-content; font-size: 12px; font-weight: 600; horizontal-alignment: center; vertical-alignment: center; }
                    }
                }
            }
        }
    }

    export component AppWindow inherits Window {
        title: root.document-title != "" ? root.document-title + " — BarePDF" : "BarePDF";
        icon: @image-url("../../../assets/logo.svg");
        preferred-width: 1200px;
        preferred-height: 860px;
        min-width: 760px;
        min-height: 520px;
        background: ThemeTokens.window;

        in property <string> document-title: "";
        in property <string> status-text: "Ready";
        in-out property <string> current-page-str: "1";
        in property <string> total-pages-str: "0";
        in property <string> zoom-str: "100%";
        in property <image> page-bitmap;
        in property <length> page-display-width: 800px;
        in property <length> page-display-height: 1040px;
        in property <length> document-total-height: 1040px;
        in property <[PageItem]> visible-pages: [];
        in property <[ThumbnailItem]> thumbnail-items: [];
        in property <[OutlineItem]> outline-items: [];
        in property <[RecentFileItem]> recent-files: [];
        in-out property <length> current-scroll-y: 0px;
        in-out property <length> thumbnail-scroll-y: 0px;
        out property <length> pdf-viewport-width: root.window-mode == 2 ? root.width : root.width - (root.sidebar-visible && root.has-document ? 248px : 0px);
        out property <length> pdf-viewport-height: root.window-mode != 0 ? root.height : root.height - 88px - (root.banner-visible ? 42px : 0px);
        out property <length> thumbnail-viewport-height: root.pdf-viewport-height - 56px;
        in property <bool> has-document: false;
        in property <bool> has-selection: false;
        in-out property <bool> password-required: false;
        in property <string> password-error: "";
        in-out property <bool> settings-open: false;
        in-out property <bool> context-menu-open: false;
        in-out property <length> context-menu-x: 0px;
        in-out property <length> context-menu-y: 0px;
        in property <string> protected-file-name: "";
        in-out property <bool> sidebar-visible: true;
        in-out property <int> sidebar-tab: 0;
        in-out property <int> window-mode: 0;
        in property <int> view-mode: 0;
        in property <string> view-mode-label: "Continuous";
        in property <int> current-language: 0;
        in property <int> current-theme: 0;
        in property <bool> banner-visible: false;
        in property <string> banner-text: "";
        in property <bool> banner-can-retry: false;

        in property <string> text-open: "Open PDF";
        in property <string> text-sidebar: "Sidebar";
        in property <string> text-thumbnails: "Thumbnails";
        in property <string> text-outline: "Outline";
        in property <string> text-view: "View";
        in property <string> text-zoom-in: "Zoom In";
        in property <string> text-zoom-out: "Zoom Out";
        in property <string> text-fit-width: "Fit Width";
        in property <string> text-fit-page: "Fit Page";
        in property <string> text-actual-size: "Actual Size";
        in property <string> text-fullscreen: "Full Screen";
        in property <string> text-presentation: "Presentation";
        in property <string> text-settings: "Settings";
        in property <string> text-copy: "Copy";
        in property <string> text-select-all: "Select All";
        in property <string> text-close: "Close";
        in property <string> text-empty-title: "No document loaded";
        in property <string> text-empty-desc: "Open or drop a PDF to begin reading.";
        in property <string> text-no-outline: "This document has no outline.";
        in property <string> text-recent: "Recent files";
        in property <string> text-retry: "Retry";
        in property <string> text-dismiss: "Dismiss";
        in property <string> text-loading: "Loading";

        callback request-open-file();
        callback request-next-page();
        callback request-prev-page();
        callback request-first-page();
        callback request-last-page();
        callback request-go-to-page(string);
        callback request-zoom-in();
        callback request-zoom-out();
        callback request-fit-width();
        callback request-fit-page();
        callback request-actual-size();
        callback request-toggle-sidebar();
        callback request-sidebar-tab(int);
        callback request-toggle-outline(int);
        callback request-toggle-fullscreen();
        callback request-presentation-mode();
        callback request-exit-special-mode();
        callback request-unlock-password(string);
        callback request-select-page(int);
        callback request-toggle-view-mode();
        callback request-change-language(int);
        callback request-change-theme(int);
        callback request-copy();
        callback request-select-all();
        callback request-open-recent(string);
        callback request-drop(data-transfer);
        callback request-dismiss-banner();
        callback request-retry();
        callback pointer-down(int, length, length, int);
        callback pointer-move(int, length, length);
        callback pointer-up(int, length, length);

        FocusScope {
            key-pressed(event) => {
                if (event.text == "\u{001b}") {
                    if (root.context-menu-open) { root.context-menu-open = false; return accept; }
                    if (root.settings-open) { root.settings-open = false; return accept; }
                    root.request-exit-special-mode(); return accept;
                }
                if (event.text == "\u{f11}" || event.text == "F11") { root.request-toggle-fullscreen(); return accept; }
                if (event.text == "\u{f5}" || event.text == "F5") { root.request-presentation-mode(); return accept; }
                if (event.text == Key.Home) { root.request-first-page(); return accept; }
                if (event.text == Key.End) { root.request-last-page(); return accept; }
                if (event.text == Key.PageDown || event.text == Key.DownArrow || event.text == Key.RightArrow) { root.request-next-page(); return accept; }
                if (event.text == Key.PageUp || event.text == Key.UpArrow || event.text == Key.LeftArrow) { root.request-prev-page(); return accept; }
                if (event.modifiers.control && (event.text == "c" || event.text == "C")) { root.request-copy(); return accept; }
                if (event.modifiers.control && (event.text == "a" || event.text == "A")) { root.request-select-all(); return accept; }
                if (event.modifiers.control && (event.text == "o" || event.text == "O")) { root.request-open-file(); return accept; }
                if (event.modifiers.control && event.text == "0") { root.request-actual-size(); return accept; }
                if (event.text == "+" || event.text == "=") { root.request-zoom-in(); return accept; }
                if (event.text == "-") { root.request-zoom-out(); return accept; }
                return reject;
            }
        }

        if root.window-mode == 2 : Rectangle {
            background: #08090b;
            TouchArea { clicked => { root.request-next-page(); } }
            Rectangle {
                width: Math.min(parent.width - 40px, root.page-display-width);
                height: Math.min(parent.height - 40px, root.page-display-height);
                background: white;
                drop-shadow-blur: 22px;
                drop-shadow-color: #000000b0;
                Image { source: root.page-bitmap; width: 100%; height: 100%; image-fit: contain; }
            }
        }

        if root.window-mode != 2 : VerticalLayout {
            spacing: 0px;

            if root.window-mode != 1 : Rectangle {
                height: 64px;
                background: ThemeTokens.command;
                border-width: 1px;
                border-color: ThemeTokens.border;

                VerticalLayout {
                    padding-top: 12px;
                    padding-bottom: 12px;

                    HorizontalLayout {
                        height: 34px;
                        padding-left: 10px;
                        padding-right: 10px;
                        spacing: 5px;

                    IconButton {
                        icon: @image-url("../../../assets/icons/document_pdf_20_regular.svg");
                        label: root.text-open; tooltip: root.text-open; show-label: true; primary: true;
                        clicked => { root.request-open-file(); }
                    }
                    IconButton {
                        icon: @image-url("../../../assets/icons/panel_left_20_regular.svg");
                        label: root.text-sidebar; tooltip: root.text-sidebar; show-label: root.width >= 980px;
                        active: root.sidebar-visible; enabled: root.has-document;
                        clicked => { root.request-toggle-sidebar(); }
                    }
                    Rectangle { width: 1px; height: 22px; background: ThemeTokens.border; }
                    IconButton {
                        icon: @image-url("../../../assets/icons/chevron_left_20_regular.svg");
                        tooltip: "Previous page"; enabled: root.has-document;
                        clicked => { root.request-prev-page(); }
                    }
                    Rectangle {
                        width: 94px; height: 34px; border-radius: 6px;
                        background: ThemeTokens.control; border-width: 1px; border-color: ThemeTokens.border;
                        HorizontalLayout {
                            padding-left: 5px; padding-right: 7px; spacing: 3px;
                            LineEdit {
                                width: 45px;
                                enabled: root.has-document;
                                text <=> root.current-page-str;
                                input-type: number;
                                horizontal-alignment: right;
                                accepted => { root.request-go-to-page(self.text); }
                            }
                            Text { text: "/ " + root.total-pages-str; color: ThemeTokens.text-muted; font-size: 11px; vertical-alignment: center; }
                        }
                    }
                    IconButton {
                        icon: @image-url("../../../assets/icons/chevron_right_20_regular.svg");
                        tooltip: "Next page"; enabled: root.has-document;
                        clicked => { root.request-next-page(); }
                    }
                    Rectangle { width: 1px; height: 22px; background: ThemeTokens.border; }
                    IconButton {
                        icon: @image-url("../../../assets/icons/zoom_out_20_regular.svg");
                        tooltip: root.text-zoom-out; enabled: root.has-document;
                        clicked => { root.request-zoom-out(); }
                    }
                    Rectangle {
                        width: 58px; height: 34px; border-radius: 6px; background: ThemeTokens.control;
                        border-width: 1px; border-color: ThemeTokens.border;
                        Text { text: root.zoom-str; color: ThemeTokens.text; font-size: 11px; font-weight: 600; horizontal-alignment: center; vertical-alignment: center; }
                    }
                    IconButton {
                        icon: @image-url("../../../assets/icons/zoom_in_20_regular.svg");
                        tooltip: root.text-zoom-in; enabled: root.has-document;
                        clicked => { root.request-zoom-in(); }
                    }
                    IconButton {
                        icon: @image-url("../../../assets/icons/slide_text_20_regular.svg");
                        label: root.view-mode-label; tooltip: root.text-view; show-label: root.width >= 1120px;
                        enabled: root.has-document; clicked => { root.request-toggle-view-mode(); }
                    }
                    IconButton {
                        icon: @image-url("../../../assets/icons/arrow_fit_20_regular.svg");
                        label: root.text-fit-width; tooltip: root.text-fit-width; show-label: root.width >= 1040px;
                        enabled: root.has-document; clicked => { root.request-fit-width(); }
                    }
                    IconButton {
                        icon: @image-url("../../../assets/icons/document_fit_20_regular.svg");
                        label: root.text-fit-page; tooltip: root.text-fit-page; show-label: root.width >= 1180px;
                        enabled: root.has-document; clicked => { root.request-fit-page(); }
                    }
                    Rectangle { horizontal-stretch: 1; }
                    IconButton {
                        icon: @image-url("../../../assets/icons/full_screen_maximize_20_regular.svg");
                        tooltip: root.text-fullscreen; enabled: root.has-document;
                        clicked => { root.request-toggle-fullscreen(); }
                    }
                    IconButton {
                        icon: @image-url("../../../assets/icons/slide_text_20_regular.svg");
                        tooltip: root.text-presentation; enabled: root.has-document;
                        clicked => { root.request-presentation-mode(); }
                    }
                        settings-button := IconButton {
                            icon: @image-url("../../../assets/icons/settings_20_regular.svg");
                            tooltip: root.text-settings; active: root.settings-open;
                            clicked => { root.settings-open = !root.settings-open; }
                        }
                    }
                }
            }

            if root.banner-visible : Rectangle {
                height: 42px;
                background: ThemeTokens.dark ? #3a2618 : #fff4e8;
                border-width: 1px;
                border-color: ThemeTokens.accent.with-alpha(0.5);
                HorizontalLayout {
                    padding-left: 14px; padding-right: 9px; spacing: 8px;
                    Text { text: root.banner-text; color: ThemeTokens.text; font-size: 12px; vertical-alignment: center; overflow: elide; horizontal-stretch: 1; }
                    if root.banner-can-retry : TextButton { text: root.text-retry; clicked => { root.request-retry(); } }
                    IconButton { icon: @image-url("../../../assets/icons/dismiss_20_regular.svg"); tooltip: root.text-dismiss; clicked => { root.request-dismiss-banner(); } }
                }
            }

            HorizontalLayout {
                spacing: 0px;

                if root.sidebar-visible && root.has-document : Rectangle {
                    width: 248px;
                    background: ThemeTokens.panel;
                    border-width: 1px;
                    border-color: ThemeTokens.border;
                    VerticalLayout {
                        padding: 10px;
                        spacing: 10px;
                        Rectangle {
                            height: 36px; border-radius: 7px; background: ThemeTokens.window;
                            border-width: 1px; border-color: ThemeTokens.border;
                            HorizontalLayout {
                                padding: 3px; spacing: 3px;
                                TextButton { text: root.text-thumbnails; active: root.sidebar-tab == 0; clicked => { root.sidebar-tab = 0; root.request-sidebar-tab(0); } }
                                TextButton { text: root.text-outline; active: root.sidebar-tab == 1; clicked => { root.sidebar-tab = 1; root.request-sidebar-tab(1); } }
                            }
                        }

                        sidebar-list := Rectangle {
                            if root.sidebar-tab == 0 : ListView {
                                viewport-y <=> root.thumbnail-scroll-y;
                                for thumb in root.thumbnail-items : Rectangle {
                                    height: 188px;
                                    border-radius: 8px;
                                    background: thumb.is-selected ? ThemeTokens.selection : (thumb-touch.has-hover ? ThemeTokens.control-hover : #00000000);
                                    border-width: thumb.is-selected ? 2px : 1px;
                                    border-color: thumb.is-selected ? ThemeTokens.accent : ThemeTokens.border;
                                    thumb-touch := TouchArea { clicked => { root.request-select-page(thumb.page-index); } }
                                    Rectangle {
                                        x: (parent.width - self.width) / 2;
                                        y: 8px + (150px - self.height) / 2;
                                        width: Math.min(140px, thumb.width);
                                        height: Math.min(150px, thumb.height);
                                        background: white;
                                        border-width: 1px;
                                        border-color: #00000020;
                                        if thumb.has-bitmap : Image { source: thumb.bitmap; width: 100%; height: 100%; image-fit: contain; }
                                    }
                                    Text {
                                        x: 8px; y: 162px; width: parent.width - 16px; height: 20px;
                                        text: thumb.page-number; color: thumb.is-selected ? ThemeTokens.text : ThemeTokens.text-muted;
                                        font-size: 11px; font-weight: thumb.is-selected ? 600 : 500;
                                        horizontal-alignment: center; vertical-alignment: center;
                                    }
                                }
                            }

                            if root.sidebar-tab == 1 && root.outline-items.length == 0 : Text {
                                text: root.text-no-outline; color: ThemeTokens.text-muted; font-size: 12px;
                                wrap: word-wrap; horizontal-alignment: center; vertical-alignment: center;
                            }
                            if root.sidebar-tab == 1 && root.outline-items.length > 0 : ListView {
                                for item[index] in root.outline-items : Rectangle {
                                    height: 34px;
                                    border-radius: 5px;
                                    background: outline-touch.has-hover ? ThemeTokens.control-hover : #00000000;
                                    outline-touch := TouchArea { clicked => { root.request-toggle-outline(index); } }
                                    HorizontalLayout {
                                        padding-left: 7px + item.depth * 14px; padding-right: 7px; spacing: 5px;
                                        if item.has-children : Image {
                                            source: item.expanded ? @image-url("../../../assets/icons/chevron_down_20_regular.svg") : @image-url("../../../assets/icons/chevron_right_16_regular.svg");
                                            width: 16px; height: 16px; colorize: ThemeTokens.text-muted; image-fit: contain;
                                        }
                                        if !item.has-children : Rectangle { width: 16px; }
                                        Text { text: item.title; color: item.page-index >= 0 ? ThemeTokens.text : ThemeTokens.text-muted; font-size: 12px; vertical-alignment: center; overflow: elide; }
                                    }
                                }
                            }
                        }
                    }
                }

                workspace := Rectangle {
                    background: ThemeTokens.canvas;

                    drop-target := DropArea {
                        can-drop(event) => { return DragAction.copy; }
                        dropped(event) => { root.request-drop(event.data); return DragAction.copy; }
                    }
                    if drop-target.has-drag : Rectangle {
                        border-width: 2px; border-color: ThemeTokens.accent; background: ThemeTokens.accent.with-alpha(0.08);
                    }

                    if !root.has-document && !root.password-required : VerticalLayout {
                        alignment: center; spacing: 13px;
                        HorizontalLayout {
                            height: 42px; alignment: center;
                            Image { source: @image-url("../../../assets/logo.svg"); width: 42px; height: 42px; image-fit: contain; }
                        }
                        Text { text: "BarePDF"; color: ThemeTokens.text; font-size: 34px; font-weight: 750; horizontal-alignment: center; }
                        Text { text: root.text-empty-desc; color: ThemeTokens.text-muted; font-size: 13px; horizontal-alignment: center; }
                        Rectangle { height: 6px; }
                        HorizontalLayout {
                            height: 36px; alignment: center;
                            Rectangle {
                                width: 128px; height: 36px; border-radius: 7px; background: ThemeTokens.accent;
                                TouchArea { clicked => { root.request-open-file(); } }
                                Text { text: root.text-open; color: ThemeTokens.accent-content; font-size: 12px; font-weight: 650; horizontal-alignment: center; vertical-alignment: center; }
                            }
                        }
                        if root.recent-files.length > 0 : Text { text: root.text-recent; color: ThemeTokens.text-muted; font-size: 11px; font-weight: 600; horizontal-alignment: center; }
                        for recent in root.recent-files : HorizontalLayout {
                            height: 34px; alignment: center;
                            Rectangle {
                                width: 360px; height: 34px; border-radius: 6px;
                                background: recent-touch.has-hover ? ThemeTokens.control-hover : ThemeTokens.panel;
                                border-width: 1px; border-color: ThemeTokens.border;
                                recent-touch := TouchArea { clicked => { root.request-open-recent(recent.path); } }
                                Text { x: 12px; width: parent.width - 24px; text: recent.name; color: ThemeTokens.text; font-size: 12px; vertical-alignment: center; overflow: elide; }
                            }
                        }
                    }

                    if root.has-document && root.view-mode == 1 : ScrollView {
                        viewport-width: Math.max(self.width, root.page-display-width + 48px);
                        viewport-height: Math.max(self.height, root.page-display-height + 48px);
                        Rectangle {
                            width: root.page-display-width; height: root.page-display-height;
                            x: Math.max(24px, (parent.viewport-width - self.width) / 2);
                            y: Math.max(24px, (parent.viewport-height - self.height) / 2);
                            background: white; border-width: 1px; border-color: #00000020;
                            drop-shadow-blur: ThemeTokens.dark ? 16px : 8px; drop-shadow-color: #00000040;
                            if root.visible-pages.length > 0 && root.visible-pages[0].has-bitmap : Image { source: root.visible-pages[0].bitmap; width: 100%; height: 100%; image-fit: contain; }
                            if root.visible-pages.length > 0 : Rectangle {
                                for box in root.visible-pages[0].selection-boxes : Rectangle {
                                    x: box.x; y: box.y; width: box.width; height: box.height; background: ThemeTokens.selection;
                                }
                            }
                            if root.visible-pages.length > 0 : TouchArea {
                                pointer-event(event) => {
                                    if (event.kind == PointerEventKind.down && event.button == PointerEventButton.left) { root.pointer-down(root.visible-pages[0].page-index, self.mouse-x, self.mouse-y, 1); }
                                    if (event.kind == PointerEventKind.up && event.button == PointerEventButton.left) { root.pointer-up(root.visible-pages[0].page-index, self.mouse-x, self.mouse-y); }
                                    if (event.kind == PointerEventKind.move) { root.pointer-move(root.visible-pages[0].page-index, self.mouse-x, self.mouse-y); }
                                }
                            }
                        }
                    }

                    if root.has-document && root.view-mode == 0 : ScrollView {
                        viewport-width: Math.max(self.width, root.page-display-width + 48px);
                        viewport-height: Math.max(self.height, root.document-total-height + 48px);
                        viewport-y <=> root.current-scroll-y;
                        for page in root.visible-pages : Rectangle {
                            width: page.width; height: page.height;
                            x: Math.max(24px, (parent.viewport-width - page.width) / 2); y: page.y-offset;
                            background: white; border-width: 1px; border-color: #00000020;
                            drop-shadow-blur: ThemeTokens.dark ? 16px : 8px; drop-shadow-color: #00000040;
                            if page.has-bitmap : Image { source: page.bitmap; width: 100%; height: 100%; image-fit: contain; }
                            for box in page.selection-boxes : Rectangle { x: box.x; y: box.y; width: box.width; height: box.height; background: ThemeTokens.selection; }
                            TouchArea {
                                pointer-event(event) => {
                                    if (event.kind == PointerEventKind.down && event.button == PointerEventButton.left) { root.pointer-down(page.page-index, self.mouse-x, self.mouse-y, 1); }
                                    if (event.kind == PointerEventKind.up && event.button == PointerEventButton.left) { root.pointer-up(page.page-index, self.mouse-x, self.mouse-y); }
                                    if (event.kind == PointerEventKind.move) { root.pointer-move(page.page-index, self.mouse-x, self.mouse-y); }
                                }
                            }
                            if !page.has-bitmap : Text { text: root.text-loading + " " + page.page-number; color: #737373; font-size: 12px; horizontal-alignment: center; vertical-alignment: center; }
                        }
                    }

                    if root.password-required : PasswordPopover {
                        file-name: root.protected-file-name; error-text: root.password-error;
                        submit(password) => { root.request-unlock-password(password); }
                        cancel => { root.password-required = false; }
                    }

                    if root.settings-open : Rectangle {
                        background: #00000001;
                        TouchArea { clicked => { root.settings-open = false; } }
                        Rectangle {
                            x: parent.width - 330px; y: 8px; width: 320px; height: 250px;
                            background: ThemeTokens.panel; border-radius: 10px; border-width: 1px; border-color: ThemeTokens.border;
                            drop-shadow-blur: 20px; drop-shadow-color: #00000055;
                            TouchArea { clicked => { } }
                            VerticalLayout {
                                padding: 16px; spacing: 11px;
                                Text { text: root.text-settings; color: ThemeTokens.text; font-size: 17px; font-weight: 700; }
                                Text { text: "Language"; color: ThemeTokens.text-muted; font-size: 11px; font-weight: 600; }
                                HorizontalLayout {
                                    spacing: 5px;
                                    TextButton { text: "System"; active: root.current-language == 0; clicked => { root.request-change-language(0); } }
                                    TextButton { text: "English"; active: root.current-language == 1; clicked => { root.request-change-language(1); } }
                                    TextButton { text: "Türkçe"; active: root.current-language == 2; clicked => { root.request-change-language(2); } }
                                }
                                Text { text: "Theme"; color: ThemeTokens.text-muted; font-size: 11px; font-weight: 600; }
                                HorizontalLayout {
                                    spacing: 5px;
                                    TextButton { text: "System"; active: root.current-theme == 0; clicked => { root.request-change-theme(0); } }
                                    TextButton { text: "Light"; active: root.current-theme == 1; clicked => { root.request-change-theme(1); } }
                                    TextButton { text: "Dark"; active: root.current-theme == 2; clicked => { root.request-change-theme(2); } }
                                }
                            }
                        }
                    }
                }
            }

            if root.window-mode != 1 : Rectangle {
                height: 24px; background: ThemeTokens.command; border-width: 1px; border-color: ThemeTokens.border;
                HorizontalLayout {
                    padding-left: 11px; padding-right: 11px; alignment: space-between;
                    Text { text: root.status-text; color: ThemeTokens.text-muted; font-size: 10px; vertical-alignment: center; overflow: elide; }
                    Text { text: root.has-document ? root.view-mode-label : ""; color: ThemeTokens.text-muted; font-size: 10px; vertical-alignment: center; }
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
