use barepdf_pdf::pdfium_lifetime::{pdfium_ffi_lock, resolve_pdfium_library_path_with_policy};
use std::fs;
use tempfile::tempdir;

#[test]
fn pdfium_ffi_lock_synchronizes_and_recovers_from_poison() {
    // 1. Basic lock acquisition
    {
        let _guard = pdfium_ffi_lock();
    }

    // 2. Poison the lock in a child thread
    let handle = std::thread::spawn(|| {
        let _guard = pdfium_ffi_lock();
        panic!("simulated worker thread panic while holding pdfium_ffi_lock");
    });
    let join_result = handle.join();
    assert!(join_result.is_err(), "thread should have panicked");

    // 3. Verify poison recovery: pdfium_ffi_lock must not panic or error
    let recovered_guard = pdfium_ffi_lock();
    drop(recovered_guard);
}

#[test]
fn pdfium_library_resolution_rejects_cwd_and_fallbacks_when_disabled() {
    let dir = tempdir().expect("tempdir");
    let fake_bin_dir = dir.path().join("bin");
    fs::create_dir_all(&fake_bin_dir).expect("create fake bin dir");
    let fake_exe = fake_bin_dir.join("barepdf.exe");
    fs::write(&fake_exe, b"MZfakeexe").expect("write fake exe");

    let dll_name = "pdfium.dll";

    // Scenario A: No sibling DLL exists, fallbacks disabled (Release mode policy)
    let resolved = resolve_pdfium_library_path_with_policy(&fake_exe, dll_name, false);
    assert!(
        resolved.is_none(),
        "When fallbacks are disabled, non-sibling paths must be rejected"
    );

    // Scenario B: Sibling DLL exists next to the exe -> Accepted in both modes
    let sibling_dll = fake_bin_dir.join(dll_name);
    fs::write(&sibling_dll, b"fake dll content").expect("write fake sibling dll");

    let resolved_release = resolve_pdfium_library_path_with_policy(&fake_exe, dll_name, false);
    assert_eq!(
        resolved_release,
        Some(sibling_dll.clone()),
        "Sibling DLL must be accepted when fallbacks are disabled"
    );

    // Scenario C: No sibling DLL in bin, but DLL exists in exe.parent().parent()
    let other_dir = tempdir().expect("tempdir");
    let nested_bin = other_dir.path().join("sub").join("bin");
    fs::create_dir_all(&nested_bin).expect("create nested bin");
    let nested_exe = nested_bin.join("barepdf.exe");
    fs::write(&nested_exe, b"MZfakeexe").expect("write fake exe");
    let parent_parent_dll = other_dir.path().join("sub").join(dll_name);
    fs::write(&parent_parent_dll, b"fake parent-parent dll").expect("write parent parent dll");

    let resolved_release_c = resolve_pdfium_library_path_with_policy(&nested_exe, dll_name, false);
    assert!(
        resolved_release_c.is_none(),
        "Release policy must reject parent-parent DLL fallback"
    );

    let resolved_debug_c = resolve_pdfium_library_path_with_policy(&nested_exe, dll_name, true);
    assert_eq!(
        resolved_debug_c,
        Some(parent_parent_dll),
        "Debug policy should accept parent-parent DLL fallback"
    );
}
