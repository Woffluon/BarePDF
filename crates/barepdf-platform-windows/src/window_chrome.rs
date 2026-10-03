use raw_window_handle::{RawWindowHandle, WindowHandle};
use std::mem::size_of;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND, DWM_WINDOW_CORNER_PREFERENCE,
};
use windows_sys::Win32::UI::Controls::MARGINS;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    IsZoomed, PostMessageW, SendMessageW, ShowWindow, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT,
    HTCAPTION, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE,
    WM_CLOSE, WM_NCLBUTTONDOWN,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowHit {
    Caption,
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowCommand {
    Minimize,
    ToggleMaximize,
    Close,
}

impl WindowHit {
    #[must_use]
    pub const fn hit_test_code(self) -> usize {
        hit_test_code(self)
    }

    #[must_use]
    pub const fn from_resize_edge(edge: i32) -> Option<Self> {
        from_resize_edge(edge)
    }
}

#[must_use]
pub const fn hit_test_code(hit: WindowHit) -> usize {
    match hit {
        WindowHit::Caption => HTCAPTION as usize,
        WindowHit::Left => HTLEFT as usize,
        WindowHit::Right => HTRIGHT as usize,
        WindowHit::Top => HTTOP as usize,
        WindowHit::Bottom => HTBOTTOM as usize,
        WindowHit::TopLeft => HTTOPLEFT as usize,
        WindowHit::TopRight => HTTOPRIGHT as usize,
        WindowHit::BottomLeft => HTBOTTOMLEFT as usize,
        WindowHit::BottomRight => HTBOTTOMRIGHT as usize,
    }
}

#[must_use]
pub const fn from_resize_edge(edge: i32) -> Option<WindowHit> {
    match edge {
        0 => Some(WindowHit::Left),
        1 => Some(WindowHit::Right),
        2 => Some(WindowHit::Top),
        3 => Some(WindowHit::Bottom),
        4 => Some(WindowHit::TopLeft),
        5 => Some(WindowHit::TopRight),
        6 => Some(WindowHit::BottomLeft),
        7 => Some(WindowHit::BottomRight),
        _ => None,
    }
}

fn win32_hwnd(window: WindowHandle<'_>) -> Option<HWND> {
    if let RawWindowHandle::Win32(handle) = window.as_raw() {
        let hwnd = handle.hwnd.get() as HWND;
        (!hwnd.is_null()).then_some(hwnd)
    } else {
        None
    }
}

#[must_use]
pub fn begin_native_drag(window: WindowHandle<'_>, hit: WindowHit) -> bool {
    let Some(hwnd) = win32_hwnd(window) else {
        return false;
    };
    // SAFETY: `ReleaseCapture` has no pointer preconditions and releases any mouse capture on the
    // current UI thread before handing control to the native window manager move/resize loop.
    unsafe {
        ReleaseCapture();
    }
    // SAFETY: `window` guarantees a live Win32 `HWND` on the calling thread. `WM_NCLBUTTONDOWN`
    // takes the hit-test code in `WPARAM` and `0` in `LPARAM` with no pointer dereferences.
    unsafe {
        SendMessageW(hwnd, WM_NCLBUTTONDOWN, hit_test_code(hit), 0);
    }
    true
}

#[must_use]
pub fn send_window_command(window: WindowHandle<'_>, command: WindowCommand) -> bool {
    let Some(hwnd) = win32_hwnd(window) else {
        return false;
    };
    match command {
        WindowCommand::Minimize => {
            // SAFETY: `window` guarantees a live Win32 `HWND` on the calling UI thread.
            unsafe {
                ShowWindow(hwnd, SW_MINIMIZE);
            }
            true
        }
        WindowCommand::ToggleMaximize => {
            let cmd = if is_window_maximized(window) {
                SW_RESTORE
            } else {
                SW_MAXIMIZE
            };
            // SAFETY: `window` guarantees a live Win32 `HWND` on the calling UI thread.
            unsafe {
                ShowWindow(hwnd, cmd);
            }
            true
        }
        WindowCommand::Close => {
            // SAFETY: `window` guarantees a live Win32 `HWND`. `PostMessageW` enqueues `WM_CLOSE`
            // asynchronously without dereferencing any pointers.
            unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) != 0 }
        }
    }
}

#[must_use]
pub fn apply_frameless_style(window: WindowHandle<'_>) -> bool {
    let Some(hwnd) = win32_hwnd(window) else {
        return false;
    };
    let corner_preference: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
    // SAFETY: `hwnd` is a live Win32 window handle and `corner_preference` is a stack-allocated
    // `DWM_WINDOW_CORNER_PREFERENCE` whose exact byte size is passed to `DwmSetWindowAttribute`.
    let corner_hr = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&raw const corner_preference).cast(),
            u32::try_from(size_of::<DWM_WINDOW_CORNER_PREFERENCE>()).unwrap_or(4),
        )
    };
    let margins = MARGINS {
        cxLeftWidth: 1,
        cxRightWidth: 1,
        cyTopHeight: 1,
        cyBottomHeight: 1,
    };
    // SAFETY: `hwnd` is a live Win32 window handle and `margins` is a valid stack-allocated
    // `MARGINS` struct that lives for the duration of the synchronous call.
    let frame_hr = unsafe { DwmExtendFrameIntoClientArea(hwnd, &raw const margins) };
    corner_hr >= 0 && frame_hr >= 0
}

#[must_use]
pub fn is_window_maximized(window: WindowHandle<'_>) -> bool {
    let Some(hwnd) = win32_hwnd(window) else {
        return false;
    };
    // SAFETY: `window` guarantees a live Win32 `HWND`. `IsZoomed` inspects window state without
    // dereferencing user pointers.
    unsafe { IsZoomed(hwnd) != 0 }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_frameless_style, begin_native_drag, from_resize_edge, hit_test_code,
        is_window_maximized, send_window_command, WindowCommand, WindowHit,
    };
    use raw_window_handle::{RawWindowHandle, WebWindowHandle, WindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
        HTTOPRIGHT,
    };

    #[test]
    fn hit_test_code_maps_all_window_hit_variants_to_win32_constants() {
        assert_eq!(hit_test_code(WindowHit::Caption), HTCAPTION as usize);
        assert_eq!(hit_test_code(WindowHit::Left), HTLEFT as usize);
        assert_eq!(hit_test_code(WindowHit::Right), HTRIGHT as usize);
        assert_eq!(hit_test_code(WindowHit::Top), HTTOP as usize);
        assert_eq!(hit_test_code(WindowHit::Bottom), HTBOTTOM as usize);
        assert_eq!(hit_test_code(WindowHit::TopLeft), HTTOPLEFT as usize);
        assert_eq!(hit_test_code(WindowHit::TopRight), HTTOPRIGHT as usize);
        assert_eq!(hit_test_code(WindowHit::BottomLeft), HTBOTTOMLEFT as usize);
        assert_eq!(
            hit_test_code(WindowHit::BottomRight),
            HTBOTTOMRIGHT as usize
        );
        assert_eq!(WindowHit::Caption.hit_test_code(), HTCAPTION as usize);
    }

    #[test]
    fn from_resize_edge_maps_valid_edges_and_rejects_out_of_range() {
        assert_eq!(from_resize_edge(-1), None);
        assert_eq!(from_resize_edge(0), Some(WindowHit::Left));
        assert_eq!(from_resize_edge(1), Some(WindowHit::Right));
        assert_eq!(from_resize_edge(2), Some(WindowHit::Top));
        assert_eq!(from_resize_edge(3), Some(WindowHit::Bottom));
        assert_eq!(from_resize_edge(4), Some(WindowHit::TopLeft));
        assert_eq!(from_resize_edge(5), Some(WindowHit::TopRight));
        assert_eq!(from_resize_edge(6), Some(WindowHit::BottomLeft));
        assert_eq!(from_resize_edge(7), Some(WindowHit::BottomRight));
        assert_eq!(from_resize_edge(8), None);
        assert_eq!(WindowHit::from_resize_edge(0), Some(WindowHit::Left));
        assert_eq!(WindowHit::from_resize_edge(99), None);
    }

    #[test]
    fn non_win32_window_handles_return_false_gracefully() {
        let raw = RawWindowHandle::Web(WebWindowHandle::new(1));
        // SAFETY: `WebWindowHandle` carries an inert integer ID and is never dereferenced by our
        // Win32 window chrome functions.
        let handle = unsafe { WindowHandle::borrow_raw(raw) };

        assert!(!begin_native_drag(handle, WindowHit::Caption));
        assert!(!send_window_command(handle, WindowCommand::Minimize));
        assert!(!send_window_command(handle, WindowCommand::ToggleMaximize));
        assert!(!send_window_command(handle, WindowCommand::Close));
        assert!(!apply_frameless_style(handle));
        assert!(!is_window_maximized(handle));
    }
}
