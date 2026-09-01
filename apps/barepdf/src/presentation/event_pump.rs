use super::callbacks::{handle_print_event, refresh_merge_files, set_tool_source};
use super::state::AppState;
use super::ui::{
    begin_open, handle_render_event, install_native_file_drop, is_pdf_path, process_view_changes,
    show_banner,
};
use super::update_ui::handle_update_event;
use crate::application::PrintController;
use crate::infrastructure::UpdateEvent;
use barepdf_render::RenderScheduler;
use barepdf_ui::AppWindow;
use slint::{ComponentHandle, SharedString, Timer, TimerMode};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub(super) const ACTIVE_INTERVAL: Duration = Duration::from_millis(16);
const IDLE_INTERVAL: Duration = Duration::from_millis(250);
const EVENTS_PER_TICK: usize = 4;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum DropRoutingOutcome {
    OpenDocument(PathBuf),
    Banner(&'static str),
    ToolsMerge(Vec<PathBuf>),
    ToolsSingleSource(PathBuf),
    ToolsError(&'static str),
}

pub(super) fn route_dropped_paths(
    paths: &[PathBuf],
    tools_open: bool,
    current_tool: i32,
) -> DropRoutingOutcome {
    if tools_open && current_tool >= 0 {
        let pdf_paths: Vec<PathBuf> = paths.iter().filter(|p| is_pdf_path(p)).cloned().collect();
        if pdf_paths.is_empty() {
            DropRoutingOutcome::ToolsError("tools.error.drop_pdf")
        } else if current_tool == 0 {
            DropRoutingOutcome::ToolsMerge(pdf_paths)
        } else if pdf_paths.len() == 1 {
            DropRoutingOutcome::ToolsSingleSource(pdf_paths[0].clone())
        } else {
            DropRoutingOutcome::ToolsError("tools.error.single_source")
        }
    } else {
        match paths {
            [path] if is_pdf_path(path) => DropRoutingOutcome::OpenDocument(path.clone()),
            [_] => DropRoutingOutcome::Banner("Only PDF files are supported."),
            _ => DropRoutingOutcome::Banner("Drop exactly one PDF file."),
        }
    }
}

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
        std::sync::mpsc::Receiver<UpdateEvent>,
        Option<Rc<RefCell<PrintController>>>,
    ),
) {
    let (update_receiver, print_controller) = updates;
    let weak = window.as_weak();
    let state = state.clone();
    let scheduler = scheduler.clone();
    let preferences_path = preferences_path.to_path_buf();
    let callback_timer = timer.clone();
    let mut worker_terminated = false;
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
                let tools_open = window.get_tools_open();
                let current_tool = window.get_current_tool();
                match route_dropped_paths(&paths, tools_open, current_tool) {
                    DropRoutingOutcome::OpenDocument(path) => {
                        begin_open(path, None, &state, &scheduler, &window);
                    }
                    DropRoutingOutcome::Banner(message) => {
                        show_banner(&window, message, false);
                    }
                    DropRoutingOutcome::ToolsMerge(files) => {
                        let mut app = state.borrow_mut();
                        app.tools_merge_files.extend(files);
                        refresh_merge_files(&window, &mut app);
                        window.set_tools_error(SharedString::default());
                    }
                    DropRoutingOutcome::ToolsSingleSource(path) => {
                        let mut app = state.borrow_mut();
                        set_tool_source(&mut app, path);
                        window.set_tools_error(SharedString::default());
                    }
                    DropRoutingOutcome::ToolsError(error_key) => {
                        let lang = state.borrow().preferences.language.resolve();
                        window
                            .set_tools_error(SharedString::from(barepdf_i18n::t(lang, error_key)));
                    }
                }
            }
        }
        for _ in 0..EVENTS_PER_TICK {
            let Ok(event) = update_receiver.try_recv() else {
                break;
            };
            had_activity = true;
            handle_update_event(event, &window, &state);
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

    #[test]
    fn routes_dropped_paths_when_tools_is_open() {
        let pdf1 = PathBuf::from("doc1.pdf");
        let pdf2 = PathBuf::from("doc2.pdf");
        let txt = PathBuf::from("note.txt");

        // Merge tool (tool 0) accepts multiple PDFs
        assert_eq!(
            route_dropped_paths(&[pdf1.clone(), pdf2.clone()], true, 0),
            DropRoutingOutcome::ToolsMerge(vec![pdf1.clone(), pdf2.clone()])
        );

        // Split tool (tool 1) accepts 1 PDF
        assert_eq!(
            route_dropped_paths(std::slice::from_ref(&pdf1), true, 1),
            DropRoutingOutcome::ToolsSingleSource(pdf1.clone())
        );

        // Split tool rejects multiple PDFs
        assert_eq!(
            route_dropped_paths(&[pdf1.clone(), pdf2.clone()], true, 1),
            DropRoutingOutcome::ToolsError("tools.error.single_source")
        );

        // Convert tool (tool 4) accepts 1 PDF
        assert_eq!(
            route_dropped_paths(std::slice::from_ref(&pdf1), true, 4),
            DropRoutingOutcome::ToolsSingleSource(pdf1.clone())
        );

        // Non-PDF dropped into tools
        assert_eq!(
            route_dropped_paths(std::slice::from_ref(&txt), true, 2),
            DropRoutingOutcome::ToolsError("tools.error.drop_pdf")
        );
    }

    #[test]
    fn routes_dropped_paths_when_tools_is_closed() {
        let pdf1 = PathBuf::from("doc1.pdf");
        let pdf2 = PathBuf::from("doc2.pdf");
        let txt = PathBuf::from("note.txt");

        // Single PDF opens document
        assert_eq!(
            route_dropped_paths(std::slice::from_ref(&pdf1), false, -1),
            DropRoutingOutcome::OpenDocument(pdf1.clone())
        );

        // Non-PDF shows error banner
        assert_eq!(
            route_dropped_paths(std::slice::from_ref(&txt), false, -1),
            DropRoutingOutcome::Banner("Only PDF files are supported.")
        );

        // Multiple files show error banner
        assert_eq!(
            route_dropped_paths(&[pdf1, pdf2], false, -1),
            DropRoutingOutcome::Banner("Drop exactly one PDF file.")
        );
    }
}
