use barepdf_pdf::pdfium_lifetime::{
    pdfium_ffi_lock, resolve_pdfium_library_path_with_policy, yield_ffi_between_pages, FfiPriority,
    PriorityFfiGate,
};
use std::fs;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
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

#[test]
fn interactive_priority_acquires_ahead_of_waiting_background_tasks() {
    let gate = Arc::new(PriorityFfiGate::new());
    let order = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let (bg_entering_tx, bg_entering_rx) = mpsc::channel();

    let initial_guard = gate.acquire(FfiPriority::Background);

    let bg_gate = Arc::clone(&gate);
    let bg_order = Arc::clone(&order);
    let bg_handle = std::thread::spawn(move || {
        bg_entering_tx.send(()).expect("signal background start");
        let _guard = bg_gate.acquire(FfiPriority::Background);
        bg_order.lock().expect("lock order").push("background");
    });

    bg_entering_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("background thread started");
    std::thread::sleep(Duration::from_millis(20));

    let interactive_gate = Arc::clone(&gate);
    let interactive_order = Arc::clone(&order);
    let interactive_handle = std::thread::spawn(move || {
        let _guard = interactive_gate.acquire(FfiPriority::Interactive);
        interactive_order
            .lock()
            .expect("lock order")
            .push("interactive");
    });

    let deadline = Instant::now() + Duration::from_secs(2);
    while !gate.has_interactive_waiters() {
        assert!(
            Instant::now() < deadline,
            "interactive waiter did not register in time"
        );
        std::thread::yield_now();
    }

    drop(initial_guard);

    interactive_handle.join().expect("interactive join");
    bg_handle.join().expect("background join");

    assert!(!gate.has_interactive_waiters());
    assert_eq!(
        *order.lock().expect("lock order"),
        vec!["interactive", "background"]
    );
}

#[test]
fn yield_ffi_between_pages_hands_off_to_waiting_interactive_and_resumes_background() {
    let gate = Arc::new(PriorityFfiGate::new());
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let (page0_done_tx, page0_done_rx) = mpsc::channel();

    let bg_gate = Arc::clone(&gate);
    let bg_events = Arc::clone(&events);
    let bg_handle = std::thread::spawn(move || {
        let mut guard = Some(bg_gate.acquire(FfiPriority::Background));
        for page in 0..4_u32 {
            bg_events
                .lock()
                .expect("lock events")
                .push(format!("bg-page-{page}"));
            if page == 0 {
                page0_done_tx.send(()).expect("signal page 0 complete");
                let deadline = Instant::now() + Duration::from_secs(2);
                while !bg_gate.has_interactive_waiters() {
                    assert!(
                        Instant::now() < deadline,
                        "interactive task did not start waiting"
                    );
                    std::thread::yield_now();
                }
            }
            yield_ffi_between_pages(&bg_gate, &mut guard);
            assert!(
                guard.is_some(),
                "background guard must be re-acquired after yielding"
            );
        }
    });

    page0_done_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("background completed page 0");

    let interactive_gate = Arc::clone(&gate);
    let interactive_events = Arc::clone(&events);
    let interactive_handle = std::thread::spawn(move || {
        let _guard = interactive_gate.acquire(FfiPriority::Interactive);
        interactive_events
            .lock()
            .expect("lock events")
            .push("interactive".to_string());
    });

    interactive_handle.join().expect("interactive join");
    bg_handle.join().expect("background join");

    assert_eq!(
        *events.lock().expect("lock events"),
        vec![
            "bg-page-0",
            "interactive",
            "bg-page-1",
            "bg-page-2",
            "bg-page-3",
        ]
    );
}

#[test]
fn yield_ffi_between_pages_keeps_guard_when_no_interactive_waiter() {
    let gate = PriorityFfiGate::new();
    let mut guard = Some(gate.acquire(FfiPriority::Background));
    assert!(!gate.has_interactive_waiters());

    yield_ffi_between_pages(&gate, &mut guard);
    assert!(guard.is_some());
}

#[test]
fn priority_ffi_gate_recovers_from_panic_while_holding_interactive_guard() {
    let gate = Arc::new(PriorityFfiGate::new());
    let panic_gate = Arc::clone(&gate);

    let handle = std::thread::spawn(move || {
        let _guard = panic_gate.acquire(FfiPriority::Interactive);
        panic!("simulated panic while holding interactive guard");
    });
    assert!(handle.join().is_err());

    assert!(!gate.has_interactive_waiters());
    let bg_guard = gate.acquire(FfiPriority::Background);
    drop(bg_guard);
    let interactive_guard = gate.acquire(FfiPriority::Interactive);
    drop(interactive_guard);
}
