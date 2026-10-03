use barepdf_platform_windows::{
    apply_frameless_style, begin_native_drag, is_window_maximized, send_window_command,
    WindowCommand, WindowHit,
};
use barepdf_ui::AppWindow;
use raw_window_handle::HasWindowHandle;
use slint::ComponentHandle;

pub(crate) fn sync_window_maximized(window: &AppWindow) {
    let window_handle = window.window().window_handle();
    if let Ok(handle) = window_handle.window_handle() {
        window.set_window_maximized(is_window_maximized(handle));
    }
}

pub(crate) fn connect_window_chrome_callbacks(window: &AppWindow) {
    {
        let window_handle = window.window().window_handle();
        if let Ok(handle) = window_handle.window_handle() {
            let _ = apply_frameless_style(handle);
            window.set_window_maximized(is_window_maximized(handle));
        }
    }

    {
        let weak = window.as_weak();
        window.on_begin_drag(move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let window_handle = window.window().window_handle();
            if let Ok(handle) = window_handle.window_handle() {
                let _ = begin_native_drag(handle, WindowHit::Caption);
                window.set_window_maximized(is_window_maximized(handle));
            }
        });
    }

    {
        let weak = window.as_weak();
        window.on_begin_resize(move |edge| {
            let Some(hit) = WindowHit::from_resize_edge(edge) else {
                return;
            };
            let Some(window) = weak.upgrade() else {
                return;
            };
            let window_handle = window.window().window_handle();
            if let Ok(handle) = window_handle.window_handle() {
                let _ = begin_native_drag(handle, hit);
                window.set_window_maximized(is_window_maximized(handle));
            }
        });
    }

    {
        let weak = window.as_weak();
        window.on_minimize_window(move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let window_handle = window.window().window_handle();
            if let Ok(handle) = window_handle.window_handle() {
                let _ = send_window_command(handle, WindowCommand::Minimize);
            }
        });
    }

    {
        let weak = window.as_weak();
        window.on_toggle_maximize(move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let window_handle = window.window().window_handle();
            if let Ok(handle) = window_handle.window_handle() {
                let _ = send_window_command(handle, WindowCommand::ToggleMaximize);
                window.set_window_maximized(is_window_maximized(handle));
            }
        });
    }

    {
        let weak = window.as_weak();
        window.on_close_window(move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let window_handle = window.window().window_handle();
            if let Ok(handle) = window_handle.window_handle() {
                let _ = send_window_command(handle, WindowCommand::Close);
            }
        });
    }
}
