use super::callbacks::handle_print_event;
use super::state::AppState;
use super::ui::{
    automatic_update_check_should_run, begin_open, handle_render_event, handle_update_event,
    install_native_file_drop, is_pdf_path, process_view_changes, queue_update_check, show_banner,
    unix_timestamp,
};
use crate::application::PrintController;
use crate::infrastructure::{UpdateCommand, UpdateEvent};
use barepdf_render::RenderScheduler;
use barepdf_ui::AppWindow;
use slint::{ComponentHandle, Timer, TimerMode};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub(super) const ACTIVE_INTERVAL: Duration = Duration::from_millis(16);
const IDLE_INTERVAL: Duration = Duration::from_millis(250);
const UPDATE_DUE_POLL_INTERVAL: Duration = Duration::from_mins(5);
const EVENTS_PER_TICK: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PumpMode {
    Active,
    Idle,
}

impl PumpMode {
    const fn select(had_activity: bool, work_pending: bool) -> Self {
        if had_activity || work_pending {
            Self::Active
        } else {
            Self::Idle
        }
    }

    const fn interval(self) -> Duration {
        match self {
            Self::Active => ACTIVE_INTERVAL,
            Self::Idle => IDLE_INTERVAL,
        }
    }
}

pub(super) fn start(
    timer: &Rc<Timer>,
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    preferences_path: &Path,
    mut native_drop_receiver: Option<std::sync::mpsc::Receiver<Vec<PathBuf>>>,
    updates: (
        std::sync::mpsc::Sender<UpdateCommand>,
        std::sync::mpsc::Receiver<UpdateEvent>,
        Option<Rc<RefCell<PrintController>>>,
    ),
) {
    let (update_sender, update_receiver, print_controller) = updates;
    let weak = window.as_weak();
    let state = state.clone();
    let scheduler = scheduler.clone();
    let preferences_path = preferences_path.to_path_buf();
    let callback_timer = timer.clone();
    let mut worker_terminated = false;
    let mut next_update_due_poll = Instant::now() + UPDATE_DUE_POLL_INTERVAL;
    timer.start(TimerMode::Repeated, ACTIVE_INTERVAL, move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut had_activity = false;
        if native_drop_receiver.is_none() {
            native_drop_receiver = install_native_file_drop(&window);
        }
        if let Some(receiver) = native_drop_receiver.as_ref() {
            for _ in 0..EVENTS_PER_TICK {
                let Ok(paths) = receiver.try_recv() else {
                    break;
                };
                had_activity = true;
                match paths.as_slice() {
                    [path] if is_pdf_path(path) => {
                        begin_open(path.clone(), None, &state, &scheduler, &window);
                    }
                    [_] => show_banner(&window, "Only PDF files are supported.", false),
                    _ => show_banner(&window, "Drop exactly one PDF file.", false),
                }
            }
        }
        for _ in 0..EVENTS_PER_TICK {
            let Ok(event) = update_receiver.try_recv() else {
                break;
            };
            had_activity = true;
            handle_update_event(event, &window, &state, &update_sender);
        }
        if let Some(controller) = print_controller.as_ref() {
            for _ in 0..EVENTS_PER_TICK {
                let Some(event) = controller.borrow_mut().try_recv_event() else {
                    break;
                };
                had_activity = true;
                handle_print_event(event, &window);
            }
        }
        if Instant::now() >= next_update_due_poll {
            next_update_due_poll = Instant::now() + UPDATE_DUE_POLL_INTERVAL;
            let should_check = {
                let app = state.borrow();
                automatic_update_check_should_run(&app, unix_timestamp())
            };
            if should_check {
                queue_update_check(&update_sender, &state, &window, &preferences_path);
                had_activity = true;
            }
        }
        process_view_changes(&window, &state, &scheduler);
        if !worker_terminated {
            for _ in 0..EVENTS_PER_TICK {
                match scheduler.try_recv_event() {
                    Ok(Some(event)) => {
                        had_activity = true;
                        handle_render_event(event, &window, &state, &scheduler, &preferences_path);
                    }
                    Ok(None) => break,
                    Err(_) => {
                        worker_terminated = true;
                        show_banner(&window, "PDF worker stopped unexpectedly.", true);
                        break;
                    }
                }
            }
        }
        let print_active = print_controller
            .as_ref()
            .is_some_and(|controller| controller.borrow().is_active());
        let mode = PumpMode::select(
            had_activity,
            print_active || state.borrow().pump_requires_active(Instant::now()),
        );
        callback_timer.set_interval(mode.interval());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_and_pending_work_keep_pump_active() {
        assert_eq!(PumpMode::select(true, false), PumpMode::Active);
        assert_eq!(PumpMode::select(false, true), PumpMode::Active);
        assert_eq!(PumpMode::Active.interval(), Duration::from_millis(16));
    }

    #[test]
    fn quiescent_pump_uses_idle_interval() {
        assert_eq!(PumpMode::select(false, false), PumpMode::Idle);
        assert_eq!(PumpMode::Idle.interval(), Duration::from_millis(250));
    }
}
