use std::sync::{LazyLock, Mutex};

struct UiTestThread {
    tx: std::sync::mpsc::SyncSender<Box<dyn FnOnce() + Send>>,
}

static UI_TEST_THREAD: LazyLock<Mutex<UiTestThread>> = LazyLock::new(|| {
    let (tx, rx) = std::sync::mpsc::sync_channel::<Box<dyn FnOnce() + Send>>(0);
    std::thread::spawn(move || {
        while let Ok(job) = rx.recv() {
            job();
        }
    });
    Mutex::new(UiTestThread { tx })
});

pub(crate) fn run_on_ui_thread<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    let guard = UI_TEST_THREAD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
    guard
        .tx
        .send(Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            let _ = done_tx.send(result);
        }))
        .expect("UI test worker thread should be running");
    match done_rx
        .recv()
        .expect("UI test worker should report completion")
    {
        Ok(()) => {}
        Err(payload) => std::panic::resume_unwind(payload),
    }
}
