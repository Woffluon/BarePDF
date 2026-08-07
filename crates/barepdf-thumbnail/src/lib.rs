pub mod bitmap;
pub mod pdfium_loader;
pub mod provider;

use provider::BarePdfThumbnailProvider;
use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::{implement, Error, Interface, Result, GUID, HRESULT, IUnknown};
use windows::Win32::Foundation::{
    BOOL, CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_POINTER, E_UNEXPECTED, HMODULE,
    S_FALSE,
};
use windows::Win32::System::Com::*;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

/// Permanent CLSID assigned specifically to BarePDF Thumbnail Provider:
/// {4F7B3E21-9C8D-4E15-A2B0-8E9D6F3C1A5B}
pub const CLSID_BAREPDF_THUMBNAIL: GUID = GUID::from_u128(0x4f7b3e21_9c8d_4e15_a2b0_8e9d6f3c1a5b);

static G_HINSTANCE: AtomicIsize = AtomicIsize::new(0);

fn get_hinstance() -> HMODULE {
    HMODULE(G_HINSTANCE.load(Ordering::SeqCst) as *mut _)
}

#[implement(IClassFactory)]
struct BarePdfClassFactory;

impl IClassFactory_Impl for BarePdfClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Option<&IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> Result<()> {
        let res = std::panic::catch_unwind(|| {
            if punkouter.is_some() {
                return Err(Error::from(CLASS_E_NOAGGREGATION));
            }
            if ppvobject.is_null() || riid.is_null() {
                return Err(Error::from(E_POINTER));
            }

            let provider = BarePdfThumbnailProvider::new(get_hinstance());
            let unknown: IUnknown = provider.into();
            // SAFETY: Dereferencing valid riid and ppvobject pointers.
            unsafe { unknown.query(riid, ppvobject).ok() }
        });

        res.unwrap_or_else(|_| Err(Error::from(E_UNEXPECTED)))
    }

    fn LockServer(&self, _flock: BOOL) -> Result<()> {
        Ok(())
    }
}

/// Win32 DLL Entry Point
///
/// # Safety
/// Called by Windows loader. Standard DLL initialization.
#[no_mangle]
pub unsafe extern "system" fn DllMain(
    hinstance: HMODULE,
    dw_reason: u32,
    _reserved: *mut c_void,
) -> BOOL {
    if dw_reason == DLL_PROCESS_ATTACH {
        G_HINSTANCE.store(hinstance.0 as isize, Ordering::SeqCst);
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
    let res = std::panic::catch_unwind(|| {
        if rclsid.is_null() || riid.is_null() || ppv.is_null() {
            return E_POINTER;
        }

        // SAFETY: Pointer validity checked above.
        let target_clsid = unsafe { *rclsid };
        if target_clsid != CLSID_BAREPDF_THUMBNAIL {
            return CLASS_E_CLASSNOTAVAILABLE;
        }

        let factory: IClassFactory = BarePdfClassFactory.into();
        // SAFETY: Query interface on factory instance.
        unsafe { factory.query(riid, ppv) }
    });

    res.unwrap_or(E_UNEXPECTED)
}

/// COM export to check if DLL can be unloaded
///
/// # Safety
/// Standard Win32 COM export.
#[no_mangle]
pub unsafe extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}
