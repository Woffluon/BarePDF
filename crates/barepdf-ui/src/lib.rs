use slint::ComponentHandle;

slint::slint! {
    import { Button, VerticalBox, HorizontalBox, LineEdit, ScrollView, StandardListView, ListView } from "std-widgets.slint";

    component PasswordModal inherits Rectangle {
        in property <string> file_name: "";
        in-out property <string> password_input: "";
        callback submit_password(string);
        callback cancel();

        background: #00000080;

        Rectangle {
            width: 380px;
            height: 220px;
            background: #252526;
            border-radius: 8px;
            border-width: 1px;
            border-color: #3c3c3c;

            VerticalBox {
                padding: 24px;
                spacing: 16px;

                Text {
                    text: "Password Required";
                    font-size: 18px;
                    font-weight: 700;
                    color: #ffffff;
                }

                Text {
                    text: "The document is password protected. Enter password below:";
                    font-size: 13px;
                    color: #cccccc;
                    wrap: word-wrap;
                }

                LineEdit {
                    text <=> password_input;
                    placeholder-text: "Password";
                    accepted => {
                        submit_password(password_input);
                    }
                }

                HorizontalBox {
                    spacing: 12px;
                    alignment: end;

                    Button {
                        text: "Cancel";
                        clicked => { cancel(); }
                    }
                    Button {
                        text: "Unlock";
                        primary: true;
                        clicked => { submit_password(password_input); }
                    }
                }
            }
        }
    }

    export component AppWindow inherits Window {
        title: "BarePDF - Lightweight PDF Reader";
        preferred-width: 1100px;
        preferred-height: 800px;
        background: #1e1e1e;

        in property <string> status_text: "Ready";
        in property <string> current_page_str: "1";
        in property <string> total_pages_str: "0";
        in property <string> zoom_str: "100%";
        in property <image> page_bitmap;
        in property <bool> has_document: false;
        in-out property <bool> password_required: false;
        in property <string> protected_file_name: "";

        callback request_open_file();
        callback request_next_page();
        callback request_prev_page();
        callback request_first_page();
        callback request_last_page();
        callback request_zoom_in();
        callback request_zoom_out();
        callback request_fit_width();
        callback request_fit_page();
        callback request_toggle_fullscreen();
        callback request_presentation_mode();
        callback request_unlock_password(string);

        VerticalBox {
            padding: 0px;
            spacing: 0px;

            // Top Command Toolbar
            Rectangle {
                height: 48px;
                background: #252526;
                border-width: 1px;
                border-color: #2d2d2d;

                HorizontalBox {
                    padding-left: 12px;
                    padding-right: 12px;
                    alignment: space-between;

                    HorizontalBox {
                        spacing: 8px;

                        Button {
                            text: "Open PDF";
                            clicked => { root.request_open_file(); }
                        }

                        Rectangle { width: 1px; background: #3c3c3c; }

                        Button {
                            text: "<";
                            enabled: root.has_document;
                            clicked => { root.request_prev_page(); }
                        }

                        Text {
                            text: root.current_page_str + " / " + root.total_pages_str;
                            vertical-alignment: center;
                            color: #d4d4d4;
                        }

                        Button {
                            text: ">";
                            enabled: root.has_document;
                            clicked => { root.request_next_page(); }
                        }
                    }

                    HorizontalBox {
                        spacing: 8px;

                        Button {
                            text: "-";
                            enabled: root.has_document;
                            clicked => { root.request_zoom_out(); }
                        }

                        Text {
                            text: root.zoom_str;
                            vertical-alignment: center;
                            color: #d4d4d4;
                        }

                        Button {
                            text: "+";
                            enabled: root.has_document;
                            clicked => { root.request_zoom_in(); }
                        }

                        Button {
                            text: "Fit Width";
                            enabled: root.has_document;
                            clicked => { root.request_fit_width(); }
                        }

                        Button {
                            text: "Fit Page";
                            enabled: root.has_document;
                            clicked => { root.request_fit_page(); }
                        }
                    }

                    HorizontalBox {
                        spacing: 8px;

                        Button {
                            text: "Full Screen";
                            enabled: root.has_document;
                            clicked => { root.request_toggle_fullscreen(); }
                        }

                        Button {
                            text: "Presentation (F5)";
                            enabled: root.has_document;
                            clicked => { root.request_presentation_mode(); }
                        }
                    }
                }
            }

            // Main Document Viewport
            Rectangle {
                background: #181818;

                if (!root.has_document && !root.password_required) : VerticalBox {
                    alignment: center;
                    spacing: 12px;

                    Text {
                        text: "BarePDF";
                        font-size: 28px;
                        font-weight: 700;
                        color: #ffffff;
                        horizontal-alignment: center;
                    }

                    Text {
                        text: "Fast, focused, modern PDF reading without bloat.";
                        font-size: 14px;
                        color: #888888;
                        horizontal-alignment: center;
                    }

                    Button {
                        text: "Open Document (Ctrl+O)";
                        primary: true;
                        clicked => { root.request_open_file(); }
                    }
                }

                if (root.has_document) : ScrollView {
                    viewport-width: page_display.width;
                    viewport-height: page_display.height;

                    page_display := Image {
                        source: root.page_bitmap;
                    }
                }

                if (root.password_required) : PasswordModal {
                    file_name: root.protected_file_name;
                    submit_password(pwd) => { root.request_unlock_password(pwd); }
                    cancel => { root.password_required = false; }
                }
            }

            // Bottom Status Bar
            Rectangle {
                height: 24px;
                background: #007acc;

                HorizontalBox {
                    padding-left: 8px;
                    padding-right: 8px;

                    Text {
                        text: root.status_text;
                        font-size: 11px;
                        color: #ffffff;
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
