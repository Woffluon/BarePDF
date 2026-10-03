use barepdf_platform::PlatformError;
use raw_window_handle::{RawWindowHandle, WindowHandle};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
};
use windows_sys::Win32::UI::Shell::{
    DragAcceptFiles, DragFinish, DragQueryFileW, ShellExecuteW, HDROP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, DefWindowProcW, SetWindowLongPtrW, GWLP_WNDPROC, SW_SHOWNORMAL, WM_DROPFILES,
    WM_NCDESTROY, WNDPROC,
};

mod printing;
pub(crate) use printing::{
    create_direct_printer_device, show_print_dialog, DialogPrinter, PrinterDevice, PrinterJob,
};

struct DropTarget {
    sender: SyncSender<Vec<PathBuf>>,
    previous_window_proc: isize,
    processing: bool,
}

static DROP_TARGETS: OnceLock<Mutex<HashMap<usize, DropTarget>>> = OnceLock::new();

const MAX_DROP_FILES: u32 = 64;
const MAX_DROP_PATH_UNITS: usize = 32_768;

struct OwnedDropHandle(HDROP);

impl Drop for OwnedDropHandle {
    fn drop(&mut self) {
        // SAFETY: WM_DROPFILES transferred this non-null HDROP to the window procedure on its
        // dispatch thread. This owner calls DragFinish exactly once and never uses it afterward.
        unsafe { DragFinish(self.0) };
    }
}

pub(crate) fn executable_file_version_words(path: &Path) -> Result<(u32, u32), PlatformError> {
    let path = wide_null(path.as_os_str());
    let mut ignored = 0_u32;
    // SAFETY: `path` is a live NUL-terminated UTF-16 allocation for the full call. `ignored` is a
    // valid writable u32 pointer. This metadata query has no thread-affinity requirement.
    let byte_len = unsafe { GetFileVersionInfoSizeW(path.as_ptr(), &raw mut ignored) };
    if byte_len == 0 {
        return Err(PlatformError::Windows {
            operation: "Could not inspect Windows file version metadata",
            // SAFETY: GetLastError has no preconditions and is read immediately after failure.
            code: unsafe { windows_sys::Win32::Foundation::GetLastError() },
        });
    }
    let word_size = std::mem::size_of::<usize>();
    let word_len = (byte_len as usize)
        .checked_add(word_size - 1)
        .map(|size| size / word_size)
        .ok_or(PlatformError::InvalidData {
            operation: "Could not read Windows file version metadata",
            reason: "metadata size overflow",
        })?;
    let mut buffer = vec![0_usize; word_len];
    // SAFETY: `buffer` is aligned and writable for at least `byte_len` bytes, and both it and the
    // NUL-terminated path stay alive for the call. This metadata read is thread-independent.
    let read =
        unsafe { GetFileVersionInfoW(path.as_ptr(), 0, byte_len, buffer.as_mut_ptr().cast()) };
    if read == 0 {
        return Err(PlatformError::Windows {
            operation: "Could not read Windows file version metadata",
            // SAFETY: GetLastError has no preconditions and is read immediately after failure.
            code: unsafe { windows_sys::Win32::Foundation::GetLastError() },
        });
    }

    let root = ['\\' as u16, 0];
    let mut value = std::ptr::null_mut();
    let mut value_len = 0_u32;
    // SAFETY: `buffer` contains data initialized by GetFileVersionInfoW and stays alive while the
    // returned borrowed pointer is inspected. `root` is NUL-terminated; output pointers are valid.
    // VerQueryValueW has no thread-affinity requirement.
    let queried = unsafe {
        VerQueryValueW(
            buffer.as_ptr().cast(),
            root.as_ptr(),
            &raw mut value,
            &raw mut value_len,
        )
    };
    let value_len = usize::try_from(value_len).map_err(|_| PlatformError::InvalidData {
        operation: "Could not read Windows file version metadata",
        reason: "metadata length exceeds the supported range",
    })?;
    if queried == 0 || value.is_null() || value_len < std::mem::size_of::<VS_FIXEDFILEINFO>() {
        return Err(PlatformError::InvalidData {
            operation: "Could not read Windows file version metadata",
            reason: "metadata root is missing or truncated",
        });
    }
    // SAFETY: Successful VerQueryValueW returned a non-null, suitably aligned root pointer whose
    // reported length covers VS_FIXEDFILEINFO. `buffer` remains alive for this reference's use.
    let info = unsafe { &*value.cast::<VS_FIXEDFILEINFO>() };
    if info.dwSignature != 0xFEEF_04BD {
        return Err(PlatformError::InvalidData {
            operation: "Could not read Windows file version metadata",
            reason: "metadata signature is invalid",
        });
    }
    Ok((info.dwFileVersionMS, info.dwFileVersionLS))
}

pub(crate) fn open_url(url: &str) -> Result<(), PlatformError> {
    crate::shell::validate_url(url)?;
    let url = wide_null(OsStr::new(url));
    let verb = wide_null(OsStr::new("open"));
    // SAFETY: `verb` and `url` are live NUL-terminated UTF-16 allocations. Remaining optional
    // pointers are null by contract. ShellExecuteW may run on this calling thread.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if (result as usize) <= 32 {
        return Err(PlatformError::Windows {
            operation: "Could not open URL",
            code: u32::try_from(result as usize).unwrap_or_default(),
        });
    }
    Ok(())
}

pub(crate) fn install_file_drop(window: WindowHandle<'_>) -> Option<Receiver<Vec<PathBuf>>> {
    let hwnd = win32_hwnd(window.as_raw()).and_then(non_null_hwnd)?;
    // SAFETY: `WindowHandle` guarantees a live handle. Win32WindowHandle additionally requires
    // that the HWND belongs to the current thread, satisfying window-procedure registration.
    unsafe { register_file_drop(hwnd) }
}

fn win32_hwnd(window: RawWindowHandle) -> Option<HWND> {
    if let RawWindowHandle::Win32(handle) = window {
        Some(handle.hwnd.get() as HWND)
    } else {
        None
    }
}

fn non_null_hwnd(hwnd: HWND) -> Option<HWND> {
    (!hwnd.is_null()).then_some(hwnd)
}

unsafe fn register_file_drop(hwnd: HWND) -> Option<Receiver<Vec<PathBuf>>> {
    if hwnd.is_null() {
        return None;
    }
    let hwnd_key = hwnd as usize;
    let targets = DROP_TARGETS.get_or_init(|| Mutex::new(HashMap::new()));

    // Double registration guard: reject if HWND is already registered.
    // Release the mutex immediately so SetWindowLongPtrW does not self-deadlock if
    // Windows synchronously dispatches messages to drop_window_proc on this thread.
    {
        let guard = targets.lock().ok()?;
        if guard.contains_key(&hwnd_key) {
            return None;
        }
    }

    let (sender, receiver) = mpsc::sync_channel(8);
    // SAFETY: Caller supplies a live owner-thread HWND. Callback uses the system ABI and remains
    // linked for the process lifetime. Stored previous procedure is restored if registration fails
    // and otherwise forwarded until WM_NCDESTROY removes registry state.
    let previous_window_proc =
        unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, drop_window_proc as *const () as isize) };
    if previous_window_proc == 0 || previous_window_proc == drop_window_proc as *const () as isize {
        return None;
    }

    let mut guard = targets.lock().ok()?;
    guard.insert(
        hwnd_key,
        DropTarget {
            sender,
            previous_window_proc,
            processing: false,
        },
    );
    drop(guard);

    // SAFETY: Caller guarantees the HWND remains live and this runs on its owner thread. Passing
    // TRUE only changes shell drop acceptance; no borrowed pointer escapes.
    unsafe { DragAcceptFiles(hwnd, 1) };
    Some(receiver)
}

unsafe extern "system" fn drop_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: Windows invokes `drop_window_proc` on the HWND's dispatch thread with valid
        // message arguments; `drop_window_proc_impl` upholds the same callback preconditions.
        unsafe { drop_window_proc_impl(hwnd, message, wparam, lparam) }
    }));

    result.unwrap_or(0)
}

unsafe fn drop_window_proc_impl(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCDESTROY {
        let previous_window_proc = take_previous_window_proc(hwnd);
        // SAFETY: Windows invokes this callback on the HWND's dispatch thread. The procedure was
        // captured from this same live HWND and remains valid through current message dispatch.
        return unsafe {
            forward_previous_or_default(previous_window_proc, hwnd, message, wparam, lparam)
        };
    }

    if message == WM_DROPFILES {
        handle_drop_message_guarded(hwnd, wparam);
        return 0;
    }

    // SAFETY: Windows supplied all message arguments to this callback on the HWND's dispatch
    // thread. Registry lookup retains only the previous procedure captured from that HWND.
    unsafe { forward_window_proc(hwnd, message, wparam, lparam) }
}

fn handle_drop_message_guarded(hwnd: HWND, wparam: WPARAM) {
    let hwnd_key = hwnd as usize;
    // Reentrancy guard: prevent concurrent or reentrant drop processing on the same HWND
    {
        let Ok(mut targets) = DROP_TARGETS.get_or_init(Default::default).lock() else {
            return;
        };
        let Some(target) = targets.get_mut(&hwnd_key) else {
            return;
        };
        if target.processing {
            return;
        }
        target.processing = true;
    }

    struct ReentrancyGuard(usize);
    impl Drop for ReentrancyGuard {
        fn drop(&mut self) {
            if let Ok(mut targets) = DROP_TARGETS.get_or_init(Default::default).lock() {
                if let Some(target) = targets.get_mut(&self.0) {
                    target.processing = false;
                }
            }
        }
    }
    let _guard = ReentrancyGuard(hwnd_key);

    let _ = handle_drop_message(hwnd, WM_DROPFILES, wparam);
}

fn handle_drop_message(hwnd: HWND, message: u32, wparam: WPARAM) -> Result<(), ()> {
    if message != WM_DROPFILES {
        return Err(());
    }

    let drop_handle = OwnedDropHandle(wparam as HDROP);
    // SAFETY: For WM_DROPFILES, Windows supplies a live HDROP owned by this callback. Null output
    // buffer with size zero requests only item count and writes through no pointer.
    let count = unsafe { DragQueryFileW(drop_handle.0, u32::MAX, std::ptr::null_mut(), 0) };
    if count > MAX_DROP_FILES {
        return Err(());
    }
    let capacity = usize::try_from(count).map_err(|_| ())?;
    let mut paths = Vec::new();
    paths.try_reserve(capacity).map_err(|_| ())?;
    for index in 0..count {
        // SAFETY: `index` is below count reported for this still-live HDROP. Null output buffer and
        // zero size request length only; no pointer is written and callback thread is unchanged.
        let length = unsafe { DragQueryFileW(drop_handle.0, index, std::ptr::null_mut(), 0) };
        let length = usize::try_from(length).map_err(|_| ())?;
        let buffer_len = length
            .checked_add(1)
            .filter(|len| *len <= MAX_DROP_PATH_UNITS)
            .ok_or(())?;
        let mut buffer = Vec::new();
        buffer.try_reserve_exact(buffer_len).map_err(|_| ())?;
        buffer.resize(buffer_len, 0);
        let buffer_len_u32 = u32::try_from(buffer.len()).map_err(|_| ())?;
        // SAFETY: `buffer` is live and writable for exactly `buffer_len_u32` UTF-16 units. `index`
        // belongs to this live HDROP; no alias accesses the buffer during the owner-thread call.
        let copied =
            unsafe { DragQueryFileW(drop_handle.0, index, buffer.as_mut_ptr(), buffer_len_u32) };
        if usize::try_from(copied).map_err(|_| ())? != length {
            return Err(());
        }
        buffer.truncate(length);
        paths.push(PathBuf::from(String::from_utf16_lossy(&buffer)));
    }
    let targets = DROP_TARGETS.get().ok_or(())?.lock().map_err(|_| ())?;
    let target = targets.get(&(hwnd as usize)).ok_or(())?;
    target.sender.try_send(paths).map_err(|_| ())
}

unsafe fn forward_window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let previous_window_proc = DROP_TARGETS
        .get()
        .and_then(|targets| targets.lock().ok())
        .and_then(|targets| {
            targets
                .get(&(hwnd as usize))
                .map(|target| target.previous_window_proc)
        });
    // SAFETY: Caller is the active Windows callback for this HWND on its dispatch thread. Stored
    // procedure was captured from the same HWND and remains valid while registry entry is present.
    unsafe { forward_previous_or_default(previous_window_proc, hwnd, message, wparam, lparam) }
}

fn take_previous_window_proc(hwnd: HWND) -> Option<isize> {
    DROP_TARGETS
        .get()
        .and_then(|targets| targets.lock().ok())
        .and_then(|mut targets| {
            targets
                .remove(&(hwnd as usize))
                .map(|target| target.previous_window_proc)
        })
}

unsafe fn forward_previous_or_default(
    previous_window_proc: Option<isize>,
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match previous_window_proc {
        Some(previous) => {
            // SAFETY: `previous` came directly from SetWindowLongPtrW for this HWND and therefore
            // has WNDPROC representation, system ABI, and process lifetime. Caller is handling the
            // same live HWND and message on its Windows dispatch thread.
            let previous: WNDPROC = unsafe { std::mem::transmute(previous) };
            // SAFETY: Procedure identity and HWND/message/thread invariants are established above;
            // all arguments are forwarded unchanged from Windows.
            unsafe { CallWindowProcW(previous, hwnd, message, wparam, lparam) }
        }
        None => {
            // SAFETY: Windows supplied this live HWND and message tuple to its callback on the
            // window's dispatch thread; no Rust pointers or references cross this call.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
    }
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::{non_null_hwnd, take_previous_window_proc, win32_hwnd, DropTarget, DROP_TARGETS};
    use raw_window_handle::{RawWindowHandle, WebWindowHandle};
    use std::sync::mpsc;

    #[test]
    fn null_window_handle_is_rejected() {
        assert!(non_null_hwnd(std::ptr::null_mut()).is_none());
    }

    #[test]
    fn non_windows_handle_is_rejected() {
        assert!(win32_hwnd(RawWindowHandle::Web(WebWindowHandle::new(1))).is_none());
    }

    #[test]
    fn taking_previous_window_proc_removes_its_registry_entry() {
        const TEST_HWND: usize = usize::MAX;
        let (sender, _receiver) = mpsc::sync_channel(1);
        let targets = DROP_TARGETS.get_or_init(Default::default);
        targets.lock().unwrap().insert(
            TEST_HWND,
            DropTarget {
                sender,
                previous_window_proc: 123,
                processing: false,
            },
        );

        assert_eq!(take_previous_window_proc(TEST_HWND as _), Some(123));
        assert_eq!(take_previous_window_proc(TEST_HWND as _), None);
    }

    #[test]
    fn double_registration_is_prevented() {
        const TEST_HWND: usize = 0x5555;
        let (sender, _receiver) = mpsc::sync_channel(1);
        let targets = DROP_TARGETS.get_or_init(Default::default);
        targets.lock().unwrap().insert(
            TEST_HWND,
            DropTarget {
                sender,
                previous_window_proc: 123,
                processing: false,
            },
        );

        // Attempting to register the same HWND that is already in DROP_TARGETS must fail (return None)
        // SAFETY: `TEST_HWND` is already present in `DROP_TARGETS`, so `register_file_drop` returns
        // `None` before invoking any Win32 API on the synthetic handle.
        assert!(unsafe { super::register_file_drop(TEST_HWND as _) }.is_none());

        // Clean up
        take_previous_window_proc(TEST_HWND as _);
    }

    #[test]
    fn drop_window_proc_catches_unwind_and_returns_safe_lresult() {
        // Calling drop_window_proc on a null HWND or invalid message must safely catch any panic and return 0
        // SAFETY: Testing that `drop_window_proc` handles an unregistered null HWND with `WM_DROPFILES`
        // without dereferencing invalid pointers or unwinding across the FFI boundary.
        let res = unsafe {
            super::drop_window_proc(std::ptr::null_mut(), 0x0233 /* WM_DROPFILES */, 0, 0)
        };
        assert_eq!(res, 0);
    }
}
