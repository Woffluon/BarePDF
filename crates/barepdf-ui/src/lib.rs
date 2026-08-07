use slint::ComponentHandle;

slint::slint! {
    import { Button, LineEdit, ScrollView } from "std-widgets.slint";

    component FluentButton inherits Rectangle {
        in property <string> icon: "";
        in property <string> text: "";
        in property <bool> enabled: true;
        in property <bool> active: false;
        in property <bool> primary: false;
        callback clicked();

        height: 32px;
        min-width: root.text == "" ? 32px : 64px;
        border-radius: 4px;
        background: !root.enabled ? #00000000 :
                    root.primary ? (touch.has-hover ? #0066b8 : #0078d4) :
                    root.active ? #383838 :
                    (touch.has-hover ? #323232 : #252526);
        border-width: 1px;
        border-color: root.primary ? #0078d4 : (touch.has-hover ? #454545 : #333333);

        touch := TouchArea {
            enabled: root.enabled;
            clicked => { root.clicked(); }
        }

        HorizontalLayout {
            padding-left: 8px;
            padding-right: 8px;
            spacing: 6px;
            alignment: center;

            if root.icon != "" : Text {
                text: root.icon;
                font-size: 13px;
                color: !root.enabled ? #555555 : #ffffff;
                vertical-alignment: center;
            }
            if root.text != "" : Text {
                text: root.text;
                font-size: 12px;
                font-weight: 500;
                color: !root.enabled ? #555555 : #ffffff;
                vertical-alignment: center;
            }
        }
    }

    component PasswordModal inherits Rectangle {
        in property <string> file_name: "";
        in-out property <string> password_input: "";
        callback submit_password(string);
        callback cancel();

        background: #000000aa;

        Rectangle {
            width: 400px;
            height: 220px;
            background: #202020;
            border-radius: 8px;
            border-width: 1px;
            border-color: #383838;

            VerticalLayout {
                padding: 24px;
                spacing: 16px;

                HorizontalLayout {
                    spacing: 10px;
                    Image {
                        source: @image-url("../../../assets/logo.svg");
                        width: 28px;
                        height: 28px;
                        vertical-alignment: center;
                    }
                    Text {
                        text: "Password Required";
                        font-size: 18px;
                        font-weight: 700;
                        color: #ffffff;
                        vertical-alignment: center;
                    }
                }

                Text {
                    text: "This document is encrypted. Enter the password below to open it:";
                    font-size: 13px;
                    color: #cccccc;
                    wrap: word-wrap;
                }

                LineEdit {
                    text <=> password_input;
                    placeholder-text: "Password";
                    accepted => { submit_password(password_input); }
                }

                HorizontalLayout {
                    spacing: 12px;
                    alignment: end;

                    FluentButton {
                        text: "Cancel";
                        clicked => { cancel(); }
                    }
                    FluentButton {
                        text: "Unlock";
                        primary: true;
                        clicked => { submit_password(password_input); }
                    }
                }
            }
        }
    }

    export component AppWindow inherits Window {
        title: root.document_title != "" ? root.document_title + " — BarePDF" : "BarePDF";
        icon: @image-url("../../../assets/logo.svg");
        preferred-width: 1150px;
        preferred-height: 820px;
        background: #181818;

        in property <string> document_title: "";
        in property <string> status_text: "Ready";
        in property <string> current_page_str: "1";
        in property <string> total_pages_str: "0";
        in property <string> zoom_str: "100%";
        in property <image> page_bitmap;
        in property <length> page_display_width: 800px;
        in property <length> page_display_height: 1040px;
        in property <bool> has_document: false;
        in-out property <bool> password_required: false;
        in property <string> protected_file_name: "";
        in-out property <bool> sidebar_visible: false;
        in-out property <int> sidebar_tab: 0; // 0: Thumbnails, 1: Outline
        in-out property <int> window_mode: 0; // 0: Normal, 1: FullScreen, 2: Presentation
        in property <string> view_mode_label: "Fit Width";

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

        // Key shortcuts handler
        FocusScope {
            key-pressed(event) => {
                if (event.text == "\u{001b}") { // Esc
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

            // Top hint bar for presentation exit
            Rectangle {
                y: 12px;
                height: 28px;
                width: 220px;
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

            // Top Command Toolbar
            if (root.window_mode != 1) : Rectangle {
                height: 44px;
                background: #202020;
                border-width: 1px;
                border-color: #2b2b2b;

                HorizontalLayout {
                    padding-left: 10px;
                    padding-right: 10px;
                    spacing: 8px;
                    alignment: space-between;

                    // Left controls: Open, Sidebar, Nav
                    HorizontalLayout {
                        spacing: 6px;

                        Image {
                            source: @image-url("../../../assets/logo.svg");
                            width: 22px;
                            height: 22px;
                            vertical-alignment: center;
                        }

                        FluentButton {
                            icon: "🗁";
                            text: "Open";
                            clicked => { root.request_open_file(); }
                        }

                        FluentButton {
                            icon: "☰";
                            text: "";
                            active: root.sidebar_visible;
                            enabled: root.has_document;
                            clicked => { root.request_toggle_sidebar(); }
                        }

                        Rectangle { width: 1px; height: 20px; background: #333333; }

                        FluentButton {
                            icon: "◀";
                            text: "";
                            enabled: root.has_document;
                            clicked => { root.request_prev_page(); }
                        }

                        Rectangle {
                            height: 32px;
                            min-width: 70px;
                            background: #181818;
                            border-radius: 4px;
                            border-width: 1px;
                            border-color: #333333;

                            Text {
                                text: root.current_page_str + " / " + root.total_pages_str;
                                font-size: 12px;
                                color: #d0d0d0;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                        }

                        FluentButton {
                            icon: "▶";
                            text: "";
                            enabled: root.has_document;
                            clicked => { root.request_next_page(); }
                        }
                    }

                    // Center controls: Zoom & View modes
                    HorizontalLayout {
                        spacing: 6px;

                        FluentButton {
                            icon: "➖";
                            text: "";
                            enabled: root.has_document;
                            clicked => { root.request_zoom_out(); }
                        }

                        Rectangle {
                            height: 32px;
                            min-width: 55px;
                            background: #181818;
                            border-radius: 4px;
                            border-width: 1px;
                            border-color: #333333;

                            Text {
                                text: root.zoom_str;
                                font-size: 12px;
                                color: #d0d0d0;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                        }

                        FluentButton {
                            icon: "➕";
                            text: "";
                            enabled: root.has_document;
                            clicked => { root.request_zoom_in(); }
                        }

                        Rectangle { width: 1px; height: 20px; background: #333333; }

                        FluentButton {
                            text: "Fit Width";
                            enabled: root.has_document;
                            clicked => { root.request_fit_width(); }
                        }

                        FluentButton {
                            text: "Fit Page";
                            enabled: root.has_document;
                            clicked => { root.request_fit_page(); }
                        }
                    }

                    // Right controls: Full Screen, Presentation
                    HorizontalLayout {
                        spacing: 6px;

                        FluentButton {
                            icon: "⛶";
                            text: "Full Screen";
                            enabled: root.has_document;
                            clicked => { root.request_toggle_fullscreen(); }
                        }

                        FluentButton {
                            icon: "🗔";
                            text: "Presentation";
                            enabled: root.has_document;
                            clicked => { root.request_presentation_mode(); }
                        }
                    }
                }
            }

            // Main Workspace (Sidebar + Document Viewport)
            HorizontalLayout {
                spacing: 0px;

                // Collapsible Sidebar
                if (root.sidebar_visible && root.has_document) : Rectangle {
                    width: 220px;
                    background: #1e1e1e;
                    border-width: 1px;
                    border-color: #2b2b2b;

                    VerticalLayout {
                        padding: 8px;
                        spacing: 8px;

                        HorizontalLayout {
                            spacing: 4px;

                            FluentButton {
                                text: "Thumbnails";
                                active: root.sidebar_tab == 0;
                                clicked => { root.sidebar_tab = 0; }
                            }
                            FluentButton {
                                text: "Outline";
                                active: root.sidebar_tab == 1;
                                clicked => { root.sidebar_tab = 1; }
                            }
                        }

                        Rectangle {
                            background: #181818;
                            border-radius: 4px;

                            if root.sidebar_tab == 0 : Text {
                                text: "Page " + root.current_page_str;
                                font-size: 13px;
                                color: #888888;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }

                            if root.sidebar_tab == 1 : Text {
                                text: "No Outline available";
                                font-size: 12px;
                                color: #666666;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                        }
                    }
                }

                // Document Canvas Viewport
                Rectangle {
                    background: #121212;

                    // Empty State
                    if (!root.has_document && !root.password_required) : VerticalLayout {
                        alignment: center;
                        spacing: 16px;

                        Image {
                            source: @image-url("../../../assets/logo.svg");
                            width: 72px;
                            height: 72px;
                            horizontal-alignment: center;
                        }

                        Text {
                            text: "BarePDF";
                            font-size: 32px;
                            font-weight: 700;
                            color: #ffffff;
                            horizontal-alignment: center;
                        }

                        Text {
                            text: "Fast, focused, modern Windows PDF reader";
                            font-size: 14px;
                            color: #888888;
                            horizontal-alignment: center;
                        }

                        HorizontalLayout {
                            alignment: center;
                            FluentButton {
                                icon: "🗁";
                                text: "Open PDF (Ctrl+O)";
                                primary: true;
                                clicked => { root.request_open_file(); }
                            }
                        }
                    }

                    // Rendered Centered Viewport
                    if (root.has_document) : ScrollView {
                        viewport-width: Math.max(self.width, page_container.width + 40px);
                        viewport-height: Math.max(self.height, page_container.height + 40px);

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
                        }
                    }

                    if (root.password_required) : PasswordModal {
                        file_name: root.protected_file_name;
                        submit_password(pwd) => { root.request_unlock_password(pwd); }
                        cancel => { root.password_required = false; }
                    }
                }
            }

            // Clean Fluent Status Bar
            if (root.window_mode != 1) : Rectangle {
                height: 22px;
                background: #1a1a1a;
                border-width: 1px;
                border-color: #242424;

                HorizontalLayout {
                    padding-left: 10px;
                    padding-right: 10px;
                    alignment: space-between;

                    Text {
                        text: root.status_text;
                        font-size: 11px;
                        color: #999999;
                        vertical-alignment: center;
                    }

                    HorizontalLayout {
                        spacing: 5px;
                        Image {
                            source: @image-url("../../../assets/logo.svg");
                            width: 14px;
                            height: 14px;
                            vertical-alignment: center;
                        }
                        Text {
                            text: "BarePDF";
                            font-size: 11px;
                            color: #777777;
                            vertical-alignment: center;
                        }
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
