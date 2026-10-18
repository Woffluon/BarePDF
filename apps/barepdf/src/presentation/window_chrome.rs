#[cfg(windows)]
use barepdf_platform_windows::{
    apply_frameless_style, is_window_maximized, send_window_command, WindowCommand,
};
use barepdf_ui::AppWindow;
use i_slint_backend_winit::WinitWindowAccessor;
#[cfg(windows)]
use raw_window_handle::HasWindowHandle;
use slint::ComponentHandle;

#[cfg(windows)]
pub(crate) fn is_maximized(window: &AppWindow) -> bool {
    let window_handle = window.window().window_handle();
    if let Ok(handle) = window_handle.window_handle() {
        is_window_maximized(handle)
    } else {
        false
    }
}

#[cfg(not(windows))]
pub(crate) fn is_maximized(window: &AppWindow) -> bool {
    window
        .window()
        .with_winit_window(|w| w.is_maximized())
        .unwrap_or(false)
}

#[cfg(windows)]
pub(crate) fn apply_initial_window_state(window: &AppWindow, maximized: bool) {
    if maximized {
        let window_handle = window.window().window_handle();
        if let Ok(handle) = window_handle.window_handle() {
            let _ = send_window_command(handle, WindowCommand::Maximize);
            window.set_window_maximized(is_window_maximized(handle));
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn apply_initial_window_state(window: &AppWindow, maximized: bool) {
    if maximized {
        window.window().with_winit_window(|w| w.set_maximized(true));
        window.set_window_maximized(true);
    }
}

pub(crate) fn sync_window_maximized(window: &AppWindow) {
    window.set_window_maximized(is_maximized(window));
}

#[cfg(windows)]
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
            window.window().with_winit_window(|w| {
                let _ = w.drag_window();
            });
            let window_handle = window.window().window_handle();
            if let Ok(handle) = window_handle.window_handle() {
                window.set_window_maximized(is_window_maximized(handle));
            }
        });
    }

    {
        let weak = window.as_weak();
        window.on_begin_resize(move |edge| {
            let direction = match edge {
                0 => winit::window::ResizeDirection::West,
                1 => winit::window::ResizeDirection::East,
                2 => winit::window::ResizeDirection::North,
                3 => winit::window::ResizeDirection::South,
                4 => winit::window::ResizeDirection::NorthWest,
                5 => winit::window::ResizeDirection::NorthEast,
                6 => winit::window::ResizeDirection::SouthWest,
                7 => winit::window::ResizeDirection::SouthEast,
                _ => return,
            };
            let Some(window) = weak.upgrade() else {
                return;
            };
            window.window().with_winit_window(|w| {
                let _ = w.drag_resize_window(direction);
            });
            let window_handle = window.window().window_handle();
            if let Ok(handle) = window_handle.window_handle() {
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

#[cfg(not(windows))]
pub(crate) fn connect_window_chrome_callbacks(window: &AppWindow) {
    {
        let is_max = is_maximized(window);
        window.set_window_maximized(is_max);
    }

    {
        let weak = window.as_weak();
        window.on_begin_drag(move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            window.window().with_winit_window(|w| {
                let _ = w.drag_window();
            });
        });
    }

    {
        let weak = window.as_weak();
        window.on_begin_resize(move |edge| {
            let direction = match edge {
                0 => winit::window::ResizeDirection::West,
                1 => winit::window::ResizeDirection::East,
                2 => winit::window::ResizeDirection::North,
                3 => winit::window::ResizeDirection::South,
                4 => winit::window::ResizeDirection::NorthWest,
                5 => winit::window::ResizeDirection::NorthEast,
                6 => winit::window::ResizeDirection::SouthWest,
                7 => winit::window::ResizeDirection::SouthEast,
                _ => return,
            };
            let Some(window) = weak.upgrade() else {
                return;
            };
            window.window().with_winit_window(|w| {
                let _ = w.drag_resize_window(direction);
            });
        });
    }

    {
        let weak = window.as_weak();
        window.on_minimize_window(move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            window.window().with_winit_window(|w| {
                let _ = w.set_minimized(true);
            });
        });
    }

    {
        let weak = window.as_weak();
        window.on_toggle_maximize(move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            window.window().with_winit_window(|w| {
                let is_max = w.is_maximized();
                w.set_maximized(!is_max);
                window.set_window_maximized(!is_max);
            });
        });
    }

    {
        let weak = window.as_weak();
        window.on_close_window(move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let _ = window.hide();
        });
    }
}
