#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::undocumented_unsafe_blocks)]

pub mod bitmap;
pub mod pdfium_loader;
pub mod provider;

use provider::BarePdfThumbnailProvider;
use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use windows::core::{implement, Error, IUnknown, Interface, Result, GUID, HRESULT};
use windows::Win32::Foundation::{
    BOOL, CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_POINTER, E_UNEXPECTED, HMODULE,
    S_FALSE, S_OK,
};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

/// Permanent CLSID assigned specifically to `BarePDF` Thumbnail Provider:
/// {4F7B3E21-9C8D-4E15-A2B0-8E9D6F3C1A5B}
pub const CLSID_BAREPDF_THUMBNAIL: GUID = GUID::from_u128(0x4f7b3e21_9c8d_4e15_a2b0_8e9d6f3c1a5b);

static G_HINSTANCE: AtomicIsize = AtomicIsize::new(0);
static ACTIVE_OBJECTS: AtomicUsize = AtomicUsize::new(0);
static SERVER_LOCKS: AtomicUsize = AtomicUsize::new(0);

fn get_hinstance() -> HMODULE {
    HMODULE(G_HINSTANCE.load(Ordering::Acquire) as *mut _)
}

pub(crate) fn add_active_object() {
    ACTIVE_OBJECTS.fetch_add(1, Ordering::Release);
}

pub(crate) fn remove_active_object() {
    let _ = ACTIVE_OBJECTS.fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
        count.checked_sub(1)
    });
}

fn update_server_lock_count(lock: bool) -> Result<()> {
    if lock {
        SERVER_LOCKS.fetch_add(1, Ordering::Release);
        return Ok(());
    }

    SERVER_LOCKS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            count.checked_sub(1)
        })
        .map_err(|_| Error::from(E_UNEXPECTED))?;
    Ok(())
}

#[implement(IClassFactory)]
struct BarePdfClassFactory;

impl BarePdfClassFactory {
    fn new() -> Self {
        add_active_object();
        Self
    }
}

impl Drop for BarePdfClassFactory {
    fn drop(&mut self) {
        remove_active_object();
    }
}

#[inline]
pub(crate) fn is_aligned_ptr<T>(ptr: *const T) -> bool {
    !ptr.is_null() && (ptr as usize).is_multiple_of(std::mem::align_of::<T>())
}

#[inline]
pub(crate) fn is_aligned_mut_ptr<T>(ptr: *mut T) -> bool {
    !ptr.is_null() && (ptr as usize).is_multiple_of(std::mem::align_of::<T>())
}

impl IClassFactory_Impl for BarePdfClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Option<&IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> Result<()> {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if punkouter.is_some() {
                return Err(Error::from(CLASS_E_NOAGGREGATION));
            }
            if !is_aligned_ptr(riid) || !is_aligned_mut_ptr(ppvobject) {
                return Err(Error::from(E_POINTER));
            }

            let provider = BarePdfThumbnailProvider::new(get_hinstance());
            let unknown: IUnknown = provider.into();
            // SAFETY: Dereferencing valid, aligned riid and ppvobject pointers.
            unsafe { unknown.query(riid, ppvobject).ok() }
        }));

        res.unwrap_or_else(|_| Err(Error::from(E_UNEXPECTED)))
    }

    fn LockServer(&self, flock: BOOL) -> Result<()> {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            update_server_lock_count(flock.as_bool())
        }));
        res.unwrap_or_else(|_| Err(Error::from(E_UNEXPECTED)))
    }
}

/// Win32 DLL Entry Point
///
/// # Safety
/// Called by the Windows loader under the loader lock. During `DLL_PROCESS_ATTACH`, records the
/// module handle with `Ordering::Release` to pair with `Ordering::Acquire` in [`get_hinstance`].
/// During `DLL_PROCESS_DETACH`, `OnceLock<Pdfium>` in [`pdfium_loader`] is intentionally not
/// destroyed or unloaded (`FreeLibrary` / `FPDF_DestroyLibrary` must never be invoked inside
/// `DllMain` while the loader lock is held).
#[no_mangle]
pub unsafe extern "system" fn DllMain(
    hinstance: HMODULE,
    dw_reason: u32,
    _reserved: *mut c_void,
) -> BOOL {
    if dw_reason == DLL_PROCESS_ATTACH {
        G_HINSTANCE.store(hinstance.0 as isize, Ordering::Release);
    }
    BOOL::from(true)
}

/// COM export to request class factory
///
/// # Safety
/// `rclsid`, `riid`, and `ppv` must be valid pointers supplied by Windows COM subsystem.
#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if !is_aligned_ptr(rclsid) || !is_aligned_ptr(riid) || !is_aligned_mut_ptr(ppv) {
            return E_POINTER;
        }

        // SAFETY: Pointer validity and alignment checked above.
        let target_clsid = unsafe { *rclsid };
        if target_clsid != CLSID_BAREPDF_THUMBNAIL {
            return CLASS_E_CLASSNOTAVAILABLE;
        }

        let factory: IClassFactory = BarePdfClassFactory::new().into();
        // SAFETY: Query interface on factory instance.
        unsafe { factory.query(riid, ppv) }
    }));

    res.unwrap_or(E_UNEXPECTED)
}

/// COM export to check if DLL can be unloaded
///
/// # Safety
/// Standard Win32 COM export.
#[no_mangle]
pub unsafe extern "system" fn DllCanUnloadNow() -> HRESULT {
    let res = std::panic::catch_unwind(|| {
        if ACTIVE_OBJECTS.load(Ordering::Acquire) == 0 && SERVER_LOCKS.load(Ordering::Acquire) == 0
        {
            S_OK
        } else {
            S_FALSE
        }
    });

    res.unwrap_or(S_FALSE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_aligned_helpers() {
        assert!(!is_aligned_ptr::<GUID>(std::ptr::null()));
        assert!(!is_aligned_mut_ptr::<*mut c_void>(std::ptr::null_mut()));

        // Alignment check on misaligned raw address
        let aligned_buf = [0u8; 16];
        // SAFETY: Offsetting by 1 byte stays within the 16-byte stack buffer; pointer is not dereferenced.
        let misaligned_guid = unsafe { aligned_buf.as_ptr().add(1).cast::<GUID>() };
        assert!(!is_aligned_ptr(misaligned_guid));

        // SAFETY: Offsetting by 1 byte stays within the 16-byte stack buffer; pointer is not dereferenced.
        let misaligned_ppv = unsafe { aligned_buf.as_ptr().add(1) as *mut *mut c_void };
        assert!(!is_aligned_mut_ptr(misaligned_ppv));

        let guid = CLSID_BAREPDF_THUMBNAIL;
        assert!(is_aligned_ptr(&guid as *const GUID));

        let mut dummy: *mut c_void = std::ptr::null_mut();
        assert!(is_aligned_mut_ptr(&mut dummy as *mut *mut c_void));
    }

    #[test]
    fn test_dll_can_unload_now_catches_unwind_and_returns_s_ok_or_s_false() {
        // SAFETY: `DllCanUnloadNow` reads atomic counters and has no pointer preconditions.
        let hr = unsafe { DllCanUnloadNow() };
        assert!(hr == S_OK || hr == S_FALSE);
    }

    #[test]
    fn test_dll_get_class_object_rejects_null_and_misaligned() {
        let guid = CLSID_BAREPDF_THUMBNAIL;
        let mut ppv: *mut c_void = std::ptr::null_mut();

        // Null pointer
        assert_eq!(
            // SAFETY: Testing that `DllGetClassObject` rejects a null `rclsid` before dereferencing.
            unsafe { DllGetClassObject(std::ptr::null(), &guid, &mut ppv) },
            E_POINTER
        );

        // Misaligned pointer
        let aligned_buf = [0u8; 16];
        // SAFETY: Offsetting by 1 byte stays within the 16-byte stack buffer.
        let misaligned = unsafe { aligned_buf.as_ptr().add(1).cast::<GUID>() };
        assert_eq!(
            // SAFETY: Testing that `DllGetClassObject` rejects a misaligned `rclsid` before dereferencing.
            unsafe { DllGetClassObject(misaligned, &guid, &mut ppv) },
            E_POINTER
        );
    }

    #[test]
    fn test_lock_server_and_can_unload() {
        let factory: IClassFactory = BarePdfClassFactory::new().into();

        // SAFETY: `factory` is a valid in-process `IClassFactory` instance.
        assert!(unsafe { factory.LockServer(BOOL::from(true)) }.is_ok());
        // SAFETY: `DllCanUnloadNow` reads atomic state without pointer arguments.
        assert_eq!(unsafe { DllCanUnloadNow() }, S_FALSE);

        // SAFETY: Matching unlock call on the same valid `IClassFactory` instance.
        assert!(unsafe { factory.LockServer(BOOL::from(false)) }.is_ok());
    }
}
