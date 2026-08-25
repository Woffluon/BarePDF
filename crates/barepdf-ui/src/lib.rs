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
        out property <color> panel-elevated: dark ? #1b1e23 : #ffffff;
        out property <color> canvas: dark ? #0f1114 : #e9ebee;
        out property <color> control: dark ? #22252a : #ffffff;
        out property <color> control-hover: dark ? #2b2e34 : #f0f1f3;
        out property <color> control-pressed: dark ? #343840 : #e4e6e9;
        out property <color> text: dark ? #f4f4f5 : #1f2328;
        out property <color> text-muted: dark ? #a7abb3 : #626972;
        out property <color> border: dark ? #ffffff18 : #1f23281f;
        out property <color> accent: #f69423;
        out property <color> accent-content: #241407;
        out property <color> selection: #f694232e;
        out property <color> danger: dark ? #ffb4ab : #b42318;
        out property <color> focus: dark ? #ffd08a : #9a4f00;
        out property <length> space-1: 4px;
        out property <length> space-2: 8px;
        out property <length> space-3: 12px;
        out property <length> space-4: 16px;
        out property <length> control-height: 34px;
        out property <length> control-radius: 6px;
        out property <length> focus-width: 2px;
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

    export struct TabItem {
        id: int,
        title: string,
        is_active: bool,
        is_loading: bool,
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

        height: ThemeTokens.control-height;
        min-width: show-label ? 42px : ThemeTokens.control-height;
        border-radius: ThemeTokens.control-radius;
        background: !enabled ? #00000000
            : primary ? (touch.pressed ? #df7d13 : (touch.has-hover ? #ffa13a : ThemeTokens.accent))
            : active ? ThemeTokens.selection
            : (touch.pressed ? ThemeTokens.control-pressed : (touch.has-hover ? ThemeTokens.control-hover : #00000000));
        border-width: enabled && (primary || active || touch.has-hover) ? 1px : 0px;
        border-color: primary ? #d87912 : (active ? ThemeTokens.accent : ThemeTokens.border);
        accessible-role: button;
        accessible-label: tooltip;
        accessible-enabled: root.enabled;

        touch := TouchArea {
            enabled: root.enabled;
            clicked => { focus-scope.focus(); root.clicked(); }
        }

        focus-scope := FocusScope {
            x: 0px;
            width: 0px;
            enabled <=> root.enabled;
            key-pressed(event) => {
                if (event.text == " " || event.text == "\n") { root.clicked(); return accept; }
                return reject;
            }
        }

        HorizontalLayout {
            height: root.height;
            padding-left: root.show-label ? 10px : ThemeTokens.space-2;
            padding-right: root.show-label ? 11px : ThemeTokens.space-2;
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

        if focus-scope.has-focus && root.enabled : Rectangle {
            border-width: ThemeTokens.focus-width;
            border-color: ThemeTokens.focus;
            border-radius: root.border-radius;
        }
    }

    component TextButton inherits Rectangle {
        in property <string> text;
        in property <bool> active: false;
        in property <bool> enabled: true;
        in property <bool> primary: false;
        callback clicked();
        height: ThemeTokens.control-height;
        min-width: 52px;
        border-radius: ThemeTokens.control-radius;
        background: !enabled ? #00000000
            : primary ? (touch.pressed ? #df7d13 : (touch.has-hover ? #ffa13a : ThemeTokens.accent))
            : active ? ThemeTokens.selection
            : (touch.pressed ? ThemeTokens.control-pressed : (touch.has-hover ? ThemeTokens.control-hover : #00000000));
        border-width: active || primary ? 1px : 0px;
        border-color: primary ? #d87912 : ThemeTokens.accent;
        accessible-role: button;
        accessible-label: text;
        accessible-enabled: root.enabled;
        touch := TouchArea { enabled: root.enabled; clicked => { focus-scope.focus(); root.clicked(); } }
        focus-scope := FocusScope {
            x: 0px;
            width: 0px;
            enabled <=> root.enabled;
            key-pressed(event) => {
                if (event.text == " " || event.text == "\n") { root.clicked(); return accept; }
                return reject;
            }
        }
        Text {
            text: root.text;
            color: !root.enabled ? ThemeTokens.text-muted.with-alpha(0.55)
                : root.primary ? ThemeTokens.accent-content : ThemeTokens.text;
            font-size: 12px;
            font-weight: active ? 600 : 500;
            horizontal-alignment: center;
            vertical-alignment: center;
        }
        if focus-scope.has-focus && root.enabled : Rectangle {
            border-width: ThemeTokens.focus-width;
            border-color: ThemeTokens.focus;
            border-radius: root.border-radius;
        }
    }

    component DocumentTab inherits Rectangle {
        in property <TabItem> item;
        in property <string> close-label;
        callback activate();
        callback close();

        width: 156px;
        height: ThemeTokens.control-height;
        border-radius: ThemeTokens.control-radius;
        background: item.is_active ? ThemeTokens.panel-elevated
            : (tab-touch.has-hover ? ThemeTokens.control-hover : #00000000);
        border-width: item.is_active ? 1px : 0px;
        border-color: ThemeTokens.border;
        accessible-role: tab;
        accessible-label: item.title;

        tab-touch := TouchArea { clicked => { tab-focus.focus(); root.activate(); } }
        if root.item.is_active : Rectangle {
            x: 0px;
            y: 8px;
            width: 3px;
            height: parent.height - 16px;
            border-radius: 2px;
            background: ThemeTokens.accent;
        }
        tab-focus := FocusScope {
            x: 0px;
            width: 0px;
            key-pressed(event) => {
                if (event.text == " " || event.text == "\n") { root.activate(); return accept; }
                return reject;
            }
        }

        HorizontalLayout {
            padding-left: 9px;
            padding-right: 5px;
            spacing: 6px;

            if root.item.is_loading : Rectangle {
                width: 7px;
                height: 7px;
                border-radius: 4px;
                background: ThemeTokens.accent;
            }
            Text {
                text: root.item.title;
                color: ThemeTokens.text;
                font-size: 11px;
                font-weight: root.item.is_active ? 600 : 500;
                vertical-alignment: center;
                overflow: elide;
                horizontal-stretch: 1;
            }
            close-button := Rectangle {
                width: 22px;
                height: 22px;
                border-radius: 5px;
                background: close-touch.has-hover ? ThemeTokens.control-hover : #00000000;
                accessible-role: button;
                accessible-label: root.close-label + " " + root.item.title;
                close-touch := TouchArea { clicked => { root.close(); } }
                Image {
                    x: 3px;
                    y: 3px;
                    width: 16px;
                    height: 16px;
                    source: @image-url("../../../assets/icons/dismiss_20_regular.svg");
                    colorize: ThemeTokens.text-muted;
                    image-fit: contain;
                    accessible-role: none;
                }
            }
        }
        if tab-focus.has-focus : Rectangle {
            border-width: ThemeTokens.focus-width;
            border-color: ThemeTokens.focus;
            border-radius: root.border-radius;
        }
    }

    component PasswordPopover inherits Rectangle {
        in-out property <string> password-input: "";
        in property <string> file-name: "";
        in property <string> error-text: "";
        in property <string> title: "";
        in property <string> placeholder: "";
        in property <string> cancel-label: "";
        in property <string> unlock-label: "";
        callback submit(string);
        callback cancel();
        background: #0000008a;
        forward-focus: modal-focus;
        TouchArea { clicked => { } }
        modal-focus := FocusScope {
            width: 420px;
            height: error-text == "" ? 230px : 258px;
            forward-focus: password-field;
            key-pressed(event) => {
                if (event.text == "\u{001b}") { root.cancel(); return accept; }
                return reject;
            }
            Rectangle {
                background: ThemeTokens.panel;
                border-radius: 10px;
                border-width: 1px;
                border-color: ThemeTokens.border;
                drop-shadow-blur: 16px;
                drop-shadow-color: #00000048;
                VerticalLayout {
                    padding: 24px;
                    spacing: 13px;
                    Text { text: root.title; color: ThemeTokens.text; font-size: 19px; font-weight: 700; }
                    Text { text: root.file-name; color: ThemeTokens.text-muted; font-size: 12px; overflow: elide; }
                    if root.error-text != "" : Text { text: root.error-text; color: ThemeTokens.danger; font-size: 12px; }
                    password-field := LineEdit {
                        text <=> root.password-input;
                        placeholder-text: root.placeholder;
                        accessible-label: root.placeholder;
                        input-type: password;
                        accepted => { root.submit(root.password-input); }
                    }
                    HorizontalLayout {
                        spacing: ThemeTokens.space-2;
                        alignment: end;
                        TextButton { text: root.cancel-label; clicked => { root.cancel(); } }
                        TextButton { text: root.unlock-label; primary: true; clicked => { root.submit(root.password-input); } }
                    }
                }
            }
        }
    }

    component ToolsModal inherits Rectangle {
        in-out property <int> current-tool: -1;
        in property <[string]> merge-files: [];
        in-out property <int> selected-merge-index: -1;
        in-out property <string> page-range-input: "";
        in-out property <int> split-mode: 0;
        in-out property <int> rotation: 1;
        in property <string> error-text: "";
        in property <bool> is-working: false;
        in property <string> active-doc-title: "";
        in property <string> active-doc-pages: "";
        in property <bool> has-document: false;

        in property <string> title: "";
        in property <string> text-merge: "";
        in property <string> text-merge-desc: "";
        in property <string> text-split: "";
        in property <string> text-split-desc: "";
        in property <string> text-delete: "";
        in property <string> text-delete-desc: "";
        in property <string> text-rotate: "";
        in property <string> text-rotate-desc: "";
        in property <string> btn-add-files: "";
        in property <string> btn-move-up: "";
        in property <string> btn-move-down: "";
        in property <string> btn-remove: "";
        in property <string> btn-clear: "";
        in property <string> btn-cancel: "";
        in property <string> btn-save: "";
        in property <string> btn-execute: "";
        in property <string> label-pages: "";
        in property <string> label-split-mode: "";
        in property <string> split-extract: "";
        in property <string> split-separate: "";
        in property <string> label-rotation: "";
        in property <string> rotation-90: "";
        in property <string> rotation-180: "";
        in property <string> rotation-270: "";

        callback select-tool(int);
        callback close();
        callback merge-add-files();
        callback merge-select-file(int);
        callback merge-move-up(int);
        callback merge-move-down(int);
        callback merge-remove(int);
        callback merge-clear();
        callback merge-submit();
        callback split-submit(string, int);
        callback delete-submit(string);
        callback rotate-submit(string, int);

        background: #0000008a;
        forward-focus: modal-focus;
        TouchArea { clicked => { } }

        modal-focus := FocusScope {
            width: root.current-tool == -1 ? 520px : 560px;
            height: root.current-tool == -1 ? 380px : (root.current-tool == 0 ? 460px : 360px);
            key-pressed(event) => {
                if (event.text == "\u{001b}") {
                    if root.current-tool != -1 {
                        root.select-tool(-1);
                    } else {
                        root.close();
                    }
                    return accept;
                }
                return reject;
            }

            Rectangle {
                background: ThemeTokens.panel;
                border-radius: 12px;
                border-width: 1px;
                border-color: ThemeTokens.border;
                drop-shadow-blur: 20px;
                drop-shadow-color: #00000055;

                VerticalLayout {
                    padding: 20px;
                    spacing: 12px;

                    HorizontalLayout {
                        alignment: space-between;
                        height: 28px;
                        HorizontalLayout {
                            spacing: 8px;
                            if root.current-tool != -1 : TextButton {
                                text: "←";
                                clicked => { root.select-tool(-1); }
                            }
                            Text {
                                text: root.current-tool == -1 ? root.title :
                                      root.current-tool == 0 ? root.text-merge :
                                      root.current-tool == 1 ? root.text-split :
                                      root.current-tool == 2 ? root.text-delete : root.text-rotate;
                                color: ThemeTokens.text;
                                font-size: 18px;
                                font-weight: 700;
                                vertical-alignment: center;
                            }
                        }
                        IconButton {
                            icon: @image-url("../../../assets/icons/dismiss_20_regular.svg");
                            tooltip: root.btn-cancel;
                            clicked => { root.close(); }
                        }
                    }

                    if root.error-text != "" : Text {
                        text: root.error-text;
                        color: ThemeTokens.danger;
                        font-size: 12px;
                        wrap: word-wrap;
                    }

                    if root.current-tool == -1 : VerticalLayout {
                        spacing: 10px;
                        Rectangle {
                            height: 62px;
                            border-radius: 8px;
                            background: m-touch.has-hover ? ThemeTokens.control-hover : ThemeTokens.control;
                            border-width: 1px;
                            border-color: ThemeTokens.border;
                            m-touch := TouchArea { clicked => { root.select-tool(0); } }
                            HorizontalLayout {
                                padding-left: 16px; padding-right: 16px; alignment: space-between;
                                VerticalLayout {
                                    alignment: center; spacing: 3px;
                                    Text { text: root.text-merge; color: ThemeTokens.text; font-size: 14px; font-weight: 650; }
                                    Text { text: root.text-merge-desc; color: ThemeTokens.text-muted; font-size: 11px; overflow: elide; }
                                }
                                Text { text: "→"; color: ThemeTokens.text-muted; font-size: 16px; vertical-alignment: center; }
                            }
                        }
                        Rectangle {
                            height: 62px;
                            border-radius: 8px;
                            background: s-touch.has-hover && root.has-document ? ThemeTokens.control-hover : ThemeTokens.control;
                            border-width: 1px;
                            border-color: ThemeTokens.border;
                            opacity: root.has-document ? 1.0 : 0.45;
                            s-touch := TouchArea { enabled: root.has-document; clicked => { root.select-tool(1); } }
                            HorizontalLayout {
                                padding-left: 16px; padding-right: 16px; alignment: space-between;
                                VerticalLayout {
                                    alignment: center; spacing: 3px;
                                    Text { text: root.text-split; color: ThemeTokens.text; font-size: 14px; font-weight: 650; }
                                    Text { text: root.text-split-desc; color: ThemeTokens.text-muted; font-size: 11px; overflow: elide; }
                                }
                                Text { text: "→"; color: ThemeTokens.text-muted; font-size: 16px; vertical-alignment: center; }
                            }
                        }
                        Rectangle {
                            height: 62px;
                            border-radius: 8px;
                            background: d-touch.has-hover && root.has-document ? ThemeTokens.control-hover : ThemeTokens.control;
                            border-width: 1px;
                            border-color: ThemeTokens.border;
                            opacity: root.has-document ? 1.0 : 0.45;
                            d-touch := TouchArea { enabled: root.has-document; clicked => { root.select-tool(2); } }
                            HorizontalLayout {
                                padding-left: 16px; padding-right: 16px; alignment: space-between;
                                VerticalLayout {
                                    alignment: center; spacing: 3px;
                                    Text { text: root.text-delete; color: ThemeTokens.text; font-size: 14px; font-weight: 650; }
                                    Text { text: root.text-delete-desc; color: ThemeTokens.text-muted; font-size: 11px; overflow: elide; }
                                }
                                Text { text: "→"; color: ThemeTokens.text-muted; font-size: 16px; vertical-alignment: center; }
                            }
                        }
                        Rectangle {
                            height: 62px;
                            border-radius: 8px;
                            background: r-touch.has-hover && root.has-document ? ThemeTokens.control-hover : ThemeTokens.control;
                            border-width: 1px;
                            border-color: ThemeTokens.border;
                            opacity: root.has-document ? 1.0 : 0.45;
                            r-touch := TouchArea { enabled: root.has-document; clicked => { root.select-tool(3); } }
                            HorizontalLayout {
                                padding-left: 16px; padding-right: 16px; alignment: space-between;
                                VerticalLayout {
                                    alignment: center; spacing: 3px;
                                    Text { text: root.text-rotate; color: ThemeTokens.text; font-size: 14px; font-weight: 650; }
                                    Text { text: root.text-rotate-desc; color: ThemeTokens.text-muted; font-size: 11px; overflow: elide; }
                                }
                                Text { text: "→"; color: ThemeTokens.text-muted; font-size: 16px; vertical-alignment: center; }
                            }
                        }
                    }

                    if root.current-tool == 0 : VerticalLayout {
                        spacing: 10px;
                        HorizontalLayout {
                            spacing: 8px;
                            TextButton { text: root.btn-add-files; primary: true; clicked => { root.merge-add-files(); } }
                            TextButton { text: root.btn-move-up; enabled: root.selected-merge-index > 0; clicked => { root.merge-move-up(root.selected-merge-index); } }
                            TextButton { text: root.btn-move-down; enabled: root.selected-merge-index >= 0 && root.selected-merge-index < root.merge-files.length - 1; clicked => { root.merge-move-down(root.selected-merge-index); } }
                            TextButton { text: root.btn-remove; enabled: root.selected-merge-index >= 0; clicked => { root.merge-remove(root.selected-merge-index); } }
                            TextButton { text: root.btn-clear; enabled: root.merge-files.length > 0; clicked => { root.merge-clear(); } }
                        }
                        Rectangle {
                            height: 240px;
                            border-radius: ThemeTokens.control-radius;
                            background: ThemeTokens.control;
                            border-width: 1px;
                            border-color: ThemeTokens.border;
                            if root.merge-files.length == 0 : Text {
                                text: root.text-merge-desc;
                                color: ThemeTokens.text-muted;
                                font-size: 12px;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                            if root.merge-files.length > 0 : ListView {
                                for file-path[fidx] in root.merge-files : Rectangle {
                                    height: 32px;
                                    background: root.selected-merge-index == fidx ? ThemeTokens.selection : (f-touch.has-hover ? ThemeTokens.control-hover : #00000000);
                                    f-touch := TouchArea { clicked => { root.merge-select-file(fidx); } }
                                    HorizontalLayout {
                                        padding-left: 10px; padding-right: 10px; spacing: 8px;
                                        Text { text: (fidx + 1) + "."; color: ThemeTokens.text-muted; font-size: 11px; vertical-alignment: center; }
                                        Text { text: file-path; color: ThemeTokens.text; font-size: 12px; vertical-alignment: center; overflow: elide; horizontal-stretch: 1; }
                                    }
                                }
                            }
                        }
                        HorizontalLayout {
                            spacing: ThemeTokens.space-2;
                            alignment: end;
                            TextButton { text: root.btn-cancel; clicked => { root.close(); } }
                            TextButton {
                                text: root.btn-save;
                                primary: true;
                                enabled: root.merge-files.length >= 2 && !root.is-working;
                                clicked => { root.merge-submit(); }
                            }
                        }
                    }

                    if root.current-tool == 1 : VerticalLayout {
                        spacing: 12px;
                        Text {
                            text: root.active-doc-title + " (" + root.active-doc-pages + " pages)";
                            color: ThemeTokens.text-muted;
                            font-size: 12px;
                            overflow: elide;
                        }
                        Text { text: root.label-split-mode; color: ThemeTokens.text; font-size: 12px; font-weight: 600; }
                        HorizontalLayout {
                            spacing: 8px;
                            TextButton { text: root.split-extract; active: root.split-mode == 0; clicked => { root.split-mode = 0; } }
                            TextButton { text: root.split-separate; active: root.split-mode == 1; clicked => { root.split-mode = 1; } }
                        }
                        if root.split-mode == 0 : VerticalLayout {
                            spacing: 6px;
                            Text { text: root.label-pages; color: ThemeTokens.text; font-size: 12px; font-weight: 600; }
                            range-edit := LineEdit {
                                text <=> root.page-range-input;
                                placeholder-text: "1-3, 5, 8-10";
                                accepted => { root.split-submit(root.page-range-input, root.split-mode); }
                            }
                        }
                        Rectangle { height: 8px; }
                        HorizontalLayout {
                            spacing: ThemeTokens.space-2;
                            alignment: end;
                            TextButton { text: root.btn-cancel; clicked => { root.close(); } }
                            TextButton {
                                text: root.btn-save;
                                primary: true;
                                enabled: !root.is-working && (root.split-mode == 1 || root.page-range-input != "");
                                clicked => { root.split-submit(root.page-range-input, root.split-mode); }
                            }
                        }
                    }

                    if root.current-tool == 2 : VerticalLayout {
                        spacing: 12px;
                        Text {
                            text: root.active-doc-title + " (" + root.active-doc-pages + " pages)";
                            color: ThemeTokens.text-muted;
                            font-size: 12px;
                            overflow: elide;
                        }
                        Text { text: root.label-pages; color: ThemeTokens.text; font-size: 12px; font-weight: 600; }
                        del-edit := LineEdit {
                            text <=> root.page-range-input;
                            placeholder-text: "2, 4-6, 10";
                            accepted => { root.delete-submit(root.page-range-input); }
                        }
                        Rectangle { height: 16px; }
                        HorizontalLayout {
                            spacing: ThemeTokens.space-2;
                            alignment: end;
                            TextButton { text: root.btn-cancel; clicked => { root.close(); } }
                            TextButton {
                                text: root.btn-save;
                                primary: true;
                                enabled: !root.is-working && root.page-range-input != "";
                                clicked => { root.delete-submit(root.page-range-input); }
                            }
                        }
                    }

                    if root.current-tool == 3 : VerticalLayout {
                        spacing: 12px;
                        Text {
                            text: root.active-doc-title + " (" + root.active-doc-pages + " pages)";
                            color: ThemeTokens.text-muted;
                            font-size: 12px;
                            overflow: elide;
                        }
                        Text { text: root.label-pages + " (empty for all pages)"; color: ThemeTokens.text; font-size: 12px; font-weight: 600; }
                        rot-edit := LineEdit {
                            text <=> root.page-range-input;
                            placeholder-text: "All pages or e.g. 1, 3-5";
                        }
                        Text { text: root.label-rotation; color: ThemeTokens.text; font-size: 12px; font-weight: 600; }
                        HorizontalLayout {
                            spacing: 8px;
                            TextButton { text: root.rotation-90; active: root.rotation == 1; clicked => { root.rotation = 1; } }
                            TextButton { text: root.rotation-180; active: root.rotation == 2; clicked => { root.rotation = 2; } }
                            TextButton { text: root.rotation-270; active: root.rotation == 3; clicked => { root.rotation = 3; } }
                        }
                        Rectangle { height: 8px; }
                        HorizontalLayout {
                            spacing: ThemeTokens.space-2;
                            alignment: end;
                            TextButton { text: root.btn-cancel; clicked => { root.close(); } }
                            TextButton {
                                text: root.btn-save;
                                primary: true;
                                enabled: !root.is-working;
                                clicked => { root.rotate-submit(root.page-range-input, root.rotation); }
                            }
                        }
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
        in property <int> zoom-mode: 0;
        property <bool> zoom-dirty: false;
        in property <image> page-bitmap;
        in property <length> page-display-width: 800px;
        in property <length> page-display-height: 1040px;
        in property <length> document-total-height: 1040px;
        in property <[PageItem]> visible-pages: [];
        in property <[ThumbnailItem]> thumbnail-items: [];
        in property <[OutlineItem]> outline-items: [];
        in property <[RecentFileItem]> recent-files: [];
        in property <[TabItem]> tab-items: [];
        in-out property <length> current-scroll-y: 0px;
        in-out property <length> thumbnail-scroll-y: 0px;
        out property <length> pdf-viewport-width: root.window-mode == 2 ? root.width : root.width - (root.sidebar-visible && root.has-document ? 248px : 0px);
        out property <length> pdf-viewport-height: root.window-mode != 0 ? root.height : root.height - 88px
            - (root.banner-visible ? 42px : 0px)
            - (root.tab-items.length > 0 ? 38px : 0px)
            - (root.print-active || root.print-status != "" ? 38px : 0px);
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
        in property <string> current-version: "";
        in property <bool> update-checks-enabled: false;
        in property <string> update-status: "";
        in property <string> update-action-label: "";
        in property <bool> update-action-enabled: false;
        in property <bool> banner-visible: false;
        in property <string> banner-text: "";
        in property <bool> banner-can-retry: false;
        in property <bool> banner-update-action: false;
        in property <string> banner-action-label: "";
        in property <bool> banner-action-enabled: false;
        in property <bool> print-active: false;
        in property <float> print-progress: 0;
        in property <string> print-status: "";
        in-out property <bool> tools-open: false;
        in-out property <int> current-tool: -1;
        in-out property <[string]> merge-files: [];
        in-out property <int> selected-merge-index: -1;
        in-out property <string> tools-page-range: "";
        in-out property <int> tools-split-mode: 0;
        in-out property <int> tools-rotation: 1;
        in property <string> tools-error: "";
        in property <bool> tools-working: false;

        in property <string> text-open: "Open PDF";
        in property <string> text-sidebar: "Sidebar";
        in property <string> text-thumbnails: "Thumbnails";
        in property <string> text-outline: "Outline";
        in property <string> text-view: "View";
        in property <string> text-zoom-in: "Zoom In";
        in property <string> text-zoom-out: "Zoom Out";
        in property <string> text-zoom: "Zoom";
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
        in property <string> text-updates: "Updates";
        in property <string> text-update-enabled: "Enabled";
        in property <string> text-update-disabled: "Disabled";
        in property <string> text-check-now: "Check now";
        in property <string> text-prev-page: "Previous page";
        in property <string> text-next-page: "Next page";
        in property <string> text-password-title: "Password Required";
        in property <string> text-password-placeholder: "Password";
        in property <string> text-password-cancel: "Cancel";
        in property <string> text-password-unlock: "Unlock";
        in property <string> text-settings-language: "Language";
        in property <string> text-settings-theme: "Theme";
        in property <string> text-settings-system: "System";
        in property <string> text-settings-english: "English";
        in property <string> text-settings-turkish: "Türkçe";
        in property <string> text-settings-light: "Light";
        in property <string> text-settings-dark: "Dark";
        in property <string> text-new-tab: "New tab";
        in property <string> text-print: "Print";
        in property <string> text-cancel-print: "Cancel";
        in property <string> text-tools: "PDF Tools";
        in property <string> text-tools-tooltip: "PDF Operations and Tools";
        in property <string> text-tools-merge: "Merge PDFs";
        in property <string> text-tools-merge-desc: "Combine multiple PDF documents into a single file.";
        in property <string> text-tools-split: "Split / Extract Pages";
        in property <string> text-tools-split-desc: "Extract specific page ranges or split every page into separate files.";
        in property <string> text-tools-delete: "Delete Pages";
        in property <string> text-tools-delete-desc: "Remove selected pages and create a new PDF document.";
        in property <string> text-tools-rotate: "Rotate Pages";
        in property <string> text-tools-rotate-desc: "Rotate pages by 90°, 180°, or 270° and save the result.";
        in property <string> text-tools-btn-add-files: "Add Files…";
        in property <string> text-tools-btn-move-up: "Move Up";
        in property <string> text-tools-btn-move-down: "Move Down";
        in property <string> text-tools-btn-remove: "Remove";
        in property <string> text-tools-btn-clear: "Clear All";
        in property <string> text-tools-btn-cancel: "Cancel";
        in property <string> text-tools-btn-save: "Save Result…";
        in property <string> text-tools-btn-execute: "Process";
        in property <string> text-tools-label-pages: "Pages (e.g. 1-3, 5, 8-10):";
        in property <string> text-tools-label-split-mode: "Split Mode:";
        in property <string> text-tools-split-extract: "Extract page range to single file";
        in property <string> text-tools-split-separate: "Split every page into separate files";
        in property <string> text-tools-label-rotation: "Rotation:";
        in property <string> text-tools-rotation-90: "90° Clockwise";
        in property <string> text-tools-rotation-180: "180°";
        in property <string> text-tools-rotation-270: "90° Counter-Clockwise";

        callback request-open-file();
        callback request-next-page();
        callback request-prev-page();
        callback request-first-page();
        callback request-last-page();
        callback request-go-to-page(string);
        callback request-zoom-in();
        callback request-zoom-out();
        callback request-set-zoom(string) -> string;
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
        callback request-change-update-checks(bool);
        callback request-check-update();
        callback request-update-action();
        callback request-copy();
        callback request-select-all();
        callback request-open-recent(string);
        callback request-drop(data-transfer);
        callback request-dismiss-banner();
        callback request-retry();
        callback request-activate-tab(int);
        callback request-close-tab(int);
        callback request-new-tab();
        callback request-print();
        callback request-cancel-print();
        callback request-toggle-tools();
        callback request-open-tool(int);
        callback request-close-tools();
        callback request-merge-add-files();
        callback request-merge-select-file(int);
        callback request-merge-move-up(int);
        callback request-merge-move-down(int);
        callback request-merge-remove-file(int);
        callback request-merge-clear();
        callback request-merge-execute();
        callback request-split-execute(string, int);
        callback request-delete-pages-execute(string);
        callback request-rotate-pages-execute(string, int);
        callback pointer-down(int, length, length, int);
        callback pointer-move(int, length, length);
        callback pointer-up(int, length, length);

        FocusScope {
            key-pressed(event) => {
                if (event.text == "\u{001b}") {
                    if (root.tools-open) {
                        if (root.current-tool != -1) {
                            root.current-tool = -1;
                        } else {
                            root.tools-open = false;
                        }
                        return accept;
                    }
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
                if (event.modifiers.control && (event.text == "p" || event.text == "P") && root.has-document && !root.print-active) { root.request-print(); return accept; }
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
                drop-shadow-blur: 12px;
                drop-shadow-color: #00000078;
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
                        height: ThemeTokens.control-height;
                        padding-left: root.width < 820px ? 4px : 10px;
                        padding-right: root.width < 820px ? 4px : 10px;
                        spacing: root.width < 820px ? 3px : 5px;

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
                        tooltip: root.text-prev-page; enabled: root.has-document;
                        clicked => { root.request-prev-page(); }
                    }
                    Rectangle {
                        width: 94px; height: ThemeTokens.control-height; border-radius: ThemeTokens.control-radius;
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
                        tooltip: root.text-next-page; enabled: root.has-document;
                        clicked => { root.request-next-page(); }
                    }
                    Rectangle { width: 1px; height: 22px; background: ThemeTokens.border; }
                    IconButton {
                        icon: @image-url("../../../assets/icons/zoom_out_20_regular.svg");
                        tooltip: root.text-zoom-out; enabled: root.has-document;
                        clicked => { root.request-zoom-out(); }
                    }
                    zoom-field := Rectangle {
                        width: 68px; height: ThemeTokens.control-height; border-radius: ThemeTokens.control-radius; background: ThemeTokens.control;
                        border-width: 1px; border-color: zoom-input.has-focus ? ThemeTokens.focus : ThemeTokens.border;
                        in property <string> canonical-zoom: root.zoom-str;
                        changed canonical-zoom => {
                            if !zoom-input.has-focus {
                                zoom-input.text = self.canonical-zoom;
                            }
                        }
                        zoom-input := LineEdit {
                            x: 7px; y: 1px; width: parent.width - 14px; height: parent.height - 2px;
                            enabled: root.has-document;
                            text: root.zoom-str;
                            font-size: 11px;
                            horizontal-alignment: center;
                            accessible-label: root.text-zoom;
                            edited => { root.zoom-dirty = true; }
                            accepted => {
                                self.text = root.request-set-zoom(self.text);
                                root.zoom-dirty = false;
                            }
                            changed has-focus => {
                                if self.has-focus {
                                    root.zoom-dirty = false;
                                    self.select-all();
                                } else if root.zoom-dirty {
                                    self.text = root.request-set-zoom(self.text);
                                    root.zoom-dirty = false;
                                } else {
                                    self.text = root.zoom-str;
                                }
                            }
                            key-pressed(event) => {
                                if event.text == "\u{001b}" {
                                    self.text = root.zoom-str;
                                    root.zoom-dirty = false;
                                    self.clear-selection();
                                    return accept;
                                }
                                return reject;
                            }
                        }
                    }
                    IconButton {
                        icon: @image-url("../../../assets/icons/zoom_in_20_regular.svg");
                        tooltip: root.text-zoom-in; enabled: root.has-document;
                        clicked => { root.request-zoom-in(); }
                    }
                    Rectangle { width: 1px; height: 22px; background: ThemeTokens.border; }
                    IconButton {
                        icon: @image-url("../../../assets/icons/slide_text_20_regular.svg");
                        label: root.view-mode-label; tooltip: root.text-view; show-label: root.width >= 1120px;
                        enabled: root.has-document; clicked => { root.request-toggle-view-mode(); }
                    }
                    IconButton {
                        icon: @image-url("../../../assets/icons/arrow_fit_20_regular.svg");
                        label: root.text-fit-width; tooltip: root.text-fit-width; show-label: root.width >= 1040px;
                        active: root.zoom-mode == 1;
                        enabled: root.has-document; clicked => { root.request-fit-width(); }
                    }
                    IconButton {
                        icon: @image-url("../../../assets/icons/document_fit_20_regular.svg");
                        label: root.text-fit-page; tooltip: root.text-fit-page; show-label: root.width >= 1180px;
                        active: root.zoom-mode == 2;
                        enabled: root.has-document; clicked => { root.request-fit-page(); }
                    }
                    Rectangle { width: 1px; height: 22px; background: ThemeTokens.border; }
                    TextButton {
                        text: root.text-print;
                        enabled: root.has-document && !root.print-active;
                        clicked => { root.request-print(); }
                    }
                    Rectangle { width: 1px; height: 22px; background: ThemeTokens.border; }
                    IconButton {
                        icon: @image-url("../../../assets/icons/document_pdf_20_regular.svg");
                        label: root.text-tools; tooltip: root.text-tools-tooltip; show-label: root.width >= 1260px;
                        active: root.tools-open;
                        clicked => { root.request-toggle-tools(); }
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

            if root.window-mode == 0 && root.tab-items.length > 0 : Rectangle {
                height: 38px;
                background: ThemeTokens.command;
                border-width: 1px;
                border-color: ThemeTokens.border;

                HorizontalLayout {
                    padding-left: 8px;
                    padding-right: 8px;
                    padding-top: 3px;
                    padding-bottom: 3px;
                    spacing: 5px;

                    tab-strip := Flickable {
                        horizontal-stretch: 1;
                        viewport-width: root.tab-items.length * 160px;
                        viewport-height: self.height;
                        interactive: true;

                        HorizontalLayout {
                            width: tab-strip.viewport-width;
                            height: tab-strip.viewport-height;
                            spacing: 4px;
                            for tab in root.tab-items : DocumentTab {
                                item: tab;
                                close-label: root.text-close;
                                activate => { root.request-activate-tab(tab.id); }
                                close => { root.request-close-tab(tab.id); }
                            }
                        }
                    }
                    new-tab-button := Rectangle {
                        width: 32px;
                        height: 32px;
                        border-radius: 6px;
                        background: new-tab-touch.has-hover && new-tab-touch.enabled ? ThemeTokens.control-hover : #00000000;
                        accessible-role: button;
                        accessible-label: root.text-new-tab;
                        new-tab-touch := TouchArea {
                            enabled: root.tab-items.length < 16;
                            clicked => { root.request-new-tab(); }
                        }
                        Text {
                            text: "+";
                            color: new-tab-touch.enabled ? ThemeTokens.text : ThemeTokens.text-muted.with-alpha(0.45);
                            font-size: 20px;
                            horizontal-alignment: center;
                            vertical-alignment: center;
                        }
                    }
                }
            }

            if root.banner-visible : Rectangle {
                height: 42px;
                background: ThemeTokens.dark ? #31251c : #fff7ef;
                border-width: 1px;
                border-color: ThemeTokens.accent.with-alpha(0.5);
                Rectangle { width: 3px; height: parent.height; background: ThemeTokens.accent; }
                HorizontalLayout {
                    padding-left: 16px; padding-right: 9px; spacing: ThemeTokens.space-2;
                    Text { text: root.banner-text; color: ThemeTokens.text; font-size: 12px; vertical-alignment: center; overflow: elide; horizontal-stretch: 1; }
                    if root.banner-update-action : TextButton {
                        text: root.banner-action-label;
                        enabled: root.banner-action-enabled;
                        primary: true;
                        clicked => { root.request-update-action(); }
                    }
                    if root.banner-can-retry : TextButton { text: root.text-retry; clicked => { root.request-retry(); } }
                    IconButton { icon: @image-url("../../../assets/icons/dismiss_20_regular.svg"); tooltip: root.text-dismiss; clicked => { root.request-dismiss-banner(); } }
                }
            }

            if root.window-mode == 0 && (root.print-active || root.print-status != "") : Rectangle {
                height: 38px;
                background: ThemeTokens.panel;
                border-width: 1px;
                border-color: ThemeTokens.border;

                Rectangle {
                    x: 0px;
                    y: parent.height - 2px;
                    width: parent.width * Math.max(0, Math.min(1, root.print-progress));
                    height: 2px;
                    background: ThemeTokens.accent;
                }
                HorizontalLayout {
                    padding-left: 12px;
                    padding-right: 8px;
                    spacing: 8px;
                    Text {
                        text: root.print-status != "" ? root.print-status : root.text-print;
                        color: ThemeTokens.text-muted;
                        font-size: 11px;
                        vertical-alignment: center;
                        overflow: elide;
                        horizontal-stretch: 1;
                    }
                    if root.print-active : TextButton {
                        text: root.text-cancel-print;
                        clicked => { root.request-cancel-print(); }
                    }
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
                            height: 36px; border-radius: ThemeTokens.control-radius; background: ThemeTokens.window;
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
                                    border-radius: ThemeTokens.control-radius;
                                    background: thumb.is-selected ? ThemeTokens.selection : (thumb-touch.has-hover ? ThemeTokens.control-hover : #00000000);
                                    border-width: 1px;
                                    border-color: ThemeTokens.border;
                                    thumb-touch := TouchArea { clicked => { root.request-select-page(thumb.page-index); } }
                                    if thumb.is-selected : Rectangle {
                                        x: 0px; y: 8px; width: 3px; height: parent.height - 16px;
                                        border-radius: 2px; background: ThemeTokens.accent;
                                    }
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
                        alignment: center; spacing: 14px;
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
                                width: 128px; height: 36px; border-radius: ThemeTokens.control-radius; background: ThemeTokens.accent;
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
                            drop-shadow-blur: ThemeTokens.dark ? 6px : 4px; drop-shadow-color: #52616e24;
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
                            drop-shadow-blur: ThemeTokens.dark ? 6px : 4px; drop-shadow-color: #52616e24;
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

                    if root.password-required : password-popup := PasswordPopover {
                        file-name: root.protected-file-name; error-text: root.password-error;
                        title: root.text-password-title;
                        placeholder: root.text-password-placeholder;
                        cancel-label: root.text-password-cancel;
                        unlock-label: root.text-password-unlock;
                        submit(password) => { root.request-unlock-password(password); password-popup.password-input = ""; }
                        cancel => { password-popup.password-input = ""; root.password-required = false; }
                    }

                    if root.settings-open : Rectangle {
                        background: #00000001;
                        TouchArea { clicked => { root.settings-open = false; } }
                        Rectangle {
                            x: parent.width - 350px; y: 8px; width: 340px; height: 402px;
                            background: ThemeTokens.panel; border-radius: 10px; border-width: 1px; border-color: ThemeTokens.border;
                            drop-shadow-blur: 12px; drop-shadow-color: #00000040;
                            TouchArea { clicked => { } }
                            VerticalLayout {
                                padding: 18px; spacing: ThemeTokens.space-3;
                                Text { text: root.text-settings; color: ThemeTokens.text; font-size: 17px; font-weight: 700; }
                                Text { text: root.text-settings-language; color: ThemeTokens.text-muted; font-size: 11px; font-weight: 600; }
                                HorizontalLayout {
                                    spacing: 5px;
                                    TextButton { text: root.text-settings-system; active: root.current-language == 0; clicked => { root.request-change-language(0); } }
                                    TextButton { text: root.text-settings-english; active: root.current-language == 1; clicked => { root.request-change-language(1); } }
                                    TextButton { text: root.text-settings-turkish; active: root.current-language == 2; clicked => { root.request-change-language(2); } }
                                }
                                Text { text: root.text-settings-theme; color: ThemeTokens.text-muted; font-size: 11px; font-weight: 600; }
                                HorizontalLayout {
                                    spacing: 5px;
                                    TextButton { text: root.text-settings-system; active: root.current-theme == 0; clicked => { root.request-change-theme(0); } }
                                    TextButton { text: root.text-settings-light; active: root.current-theme == 1; clicked => { root.request-change-theme(1); } }
                                    TextButton { text: root.text-settings-dark; active: root.current-theme == 2; clicked => { root.request-change-theme(2); } }
                                }
                                Rectangle { height: 1px; background: ThemeTokens.border; }
                                Text { text: root.text-updates + " · BarePDF v" + root.current-version; color: ThemeTokens.text-muted; font-size: 11px; font-weight: 600; }
                                HorizontalLayout {
                                    spacing: 5px;
                                    TextButton { text: root.text-update-enabled; active: root.update-checks-enabled; clicked => { root.request-change-update-checks(true); } }
                                    TextButton { text: root.text-update-disabled; active: !root.update-checks-enabled; clicked => { root.request-change-update-checks(false); } }
                                }
                                HorizontalLayout {
                                    spacing: 5px;
                                    TextButton { text: root.text-check-now; clicked => { root.request-check-update(); } }
                                    if root.update-action-label != "" : TextButton {
                                        text: root.update-action-label;
                                        enabled: root.update-action-enabled;
                                        clicked => { root.request-update-action(); }
                                    }
                                }
                                Text { text: root.update-status; color: ThemeTokens.text-muted; font-size: 10px; wrap: word-wrap; }
                            }
                        }
                    }

                    if root.tools-open : ToolsModal {
                        current-tool <=> root.current-tool;
                        merge-files: root.merge-files;
                        selected-merge-index <=> root.selected-merge-index;
                        page-range-input <=> root.tools-page-range;
                        split-mode <=> root.tools-split-mode;
                        rotation <=> root.tools-rotation;
                        error-text: root.tools-error;
                        is-working: root.tools-working;
                        active-doc-title: root.document-title;
                        active-doc-pages: root.total-pages-str;
                        has-document: root.has-document;

                        title: root.text-tools;
                        text-merge: root.text-tools-merge;
                        text-merge-desc: root.text-tools-merge-desc;
                        text-split: root.text-tools-split;
                        text-split-desc: root.text-tools-split-desc;
                        text-delete: root.text-tools-delete;
                        text-delete-desc: root.text-tools-delete-desc;
                        text-rotate: root.text-tools-rotate;
                        text-rotate-desc: root.text-tools-rotate-desc;
                        btn-add-files: root.text-tools-btn-add-files;
                        btn-move-up: root.text-tools-btn-move-up;
                        btn-move-down: root.text-tools-btn-move-down;
                        btn-remove: root.text-tools-btn-remove;
                        btn-clear: root.text-tools-btn-clear;
                        btn-cancel: root.text-tools-btn-cancel;
                        btn-save: root.text-tools-btn-save;
                        btn-execute: root.text-tools-btn-execute;
                        label-pages: root.text-tools-label-pages;
                        label-split-mode: root.text-tools-label-split-mode;
                        split-extract: root.text-tools-split-extract;
                        split-separate: root.text-tools-split-separate;
                        label-rotation: root.text-tools-label-rotation;
                        rotation-90: root.text-tools-rotation-90;
                        rotation-180: root.text-tools-rotation-180;
                        rotation-270: root.text-tools-rotation-270;

                        select-tool(tool-id) => { root.request-open-tool(tool-id); }
                        close => { root.request-close-tools(); }
                        merge-add-files => { root.request-merge-add-files(); }
                        merge-select-file(idx) => { root.request-merge-select-file(idx); }
                        merge-move-up(idx) => { root.request-merge-move-up(idx); }
                        merge-move-down(idx) => { root.request-merge-move-down(idx); }
                        merge-remove(idx) => { root.request-merge-remove-file(idx); }
                        merge-clear => { root.request-merge-clear(); }
                        merge-submit => { root.request-merge-execute(); }
                        split-submit(range, mode) => { root.request-split-execute(range, mode); }
                        delete-submit(range) => { root.request-delete-pages-execute(range); }
                        rotate-submit(range, rot) => { root.request-rotate-pages-execute(range, rot); }
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
