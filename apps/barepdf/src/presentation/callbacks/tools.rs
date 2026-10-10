use crate::infrastructure::{
    ToolEvent, ToolJobKey, ToolOperation, ToolOutcome, ToolRequest, ToolWorker,
};
use barepdf_core::{page_range::PageRangeSelection, PageCount, Rotation};
use barepdf_pdf::conversion::{ConversionDpi, ConversionFormat};
use barepdf_platform_windows::{open_url, WindowsFileDialogs};
use barepdf_render::{RenderKind, RenderScheduler};
use barepdf_ui::AppWindow;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use super::super::models::{refresh_thumbnail_model, refresh_tool_thumbnails};
use super::super::state::{ActiveToolJob, AppState};
use super::super::ui::{begin_open, show_banner};
use super::{clear_tool_password_ui, consume_ui_password, window_language, ToolKind};

pub(crate) fn refresh_merge_files(window: &AppWindow, app: &mut AppState) {
    let files = app
        .tools_merge_files
        .iter()
        .map(|path| {
            SharedString::from(
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string()),
            )
        })
        .collect::<Vec<_>>();
    let active = app
        .application
        .ready_document()
        .map(|document| (document.id(), document.path().to_path_buf()));
    let previews = app
        .tools_merge_files
        .iter()
        .map(|path| {
            active
                .as_ref()
                .filter(|(_, active_path)| active_path == path)
                .and_then(|(document, _)| {
                    app.thumbnail_images
                        .get(*document, 0, RenderKind::Thumbnail)
                })
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    window.set_merge_files(ModelRc::new(VecModel::from(files)));
    window.set_merge_first_page_images(ModelRc::new(VecModel::from(previews)));
}

fn current_tool_source(app: &AppState) -> Option<PathBuf> {
    app.tools_source_path.clone().or_else(|| {
        app.application
            .ready_document()
            .map(|document| document.path().to_path_buf())
    })
}

pub(crate) fn set_tool_source(app: &mut AppState, source: PathBuf) {
    if app.tools_source_path.as_ref() != Some(&source) {
        app.tools_source_path = Some(source);
        app.tool_source_token = app.tool_source_token.wrapping_add(1).max(1);
    }
}

pub(super) fn tool_drop_paths(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(PathBuf::from)
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
        })
        .collect()
}

pub(crate) fn selected_tool_pages(input: &str, total: u32) -> Vec<u32> {
    let Some(page_count) = PageCount::new(total) else {
        return Vec::new();
    };
    PageRangeSelection::parse(input, page_count)
        .map(|pages| pages.into_iter().map(|page| page.get() + 1).collect())
        .unwrap_or_default()
}

pub(super) fn toggle_tool_page_selection(
    existing: &str,
    page: u32,
    total: u32,
    shift: bool,
) -> String {
    if shift {
        let anchor = existing
            .split([',', '-'])
            .find_map(|item| item.trim().parse::<u32>().ok())
            .unwrap_or(page);
        format!("{}-{}", anchor.min(page), anchor.max(page))
    } else {
        let mut pages = selected_tool_pages(existing, total);
        if let Some(index) = pages.iter().position(|selected| *selected == page) {
            pages.remove(index);
        } else {
            pages.push(page);
            pages.sort_unstable();
        }
        pages
            .into_iter()
            .map(|selected| selected.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn next_tool_job_key(app: &mut AppState) -> ToolJobKey {
    let id = app.next_tool_job_id;
    app.next_tool_job_id = app.next_tool_job_id.wrapping_add(1).max(1);
    ToolJobKey::new(id, app.generation, app.tool_source_token)
}

fn queue_tool_operation(
    operation: ToolOperation,
    state: &Rc<RefCell<AppState>>,
    window: &AppWindow,
) {
    let result = (|| -> Result<(), String> {
        let mut app = state.borrow_mut();
        if app.active_tool_job.is_some() {
            Err(barepdf_i18n::t(app.preferences.language.resolve(), "tools.error.busy").to_owned())
        } else {
            if app.tool_worker.is_none() {
                app.tool_worker = Some(ToolWorker::spawn().map_err(|error| error.to_string())?);
            }
            let key = next_tool_job_key(&mut app);
            let request = ToolRequest::new(key, operation);
            let Some(worker) = app.tool_worker.as_ref() else {
                return Err("PDF tool worker is unavailable".to_owned());
            };
            let cancellation = worker.submit(request).map_err(|error| error.to_string())?;
            app.active_tool_job = Some(ActiveToolJob { key, cancellation });
            app.tool_password_source = None;
            app.wake_pump();
            Ok(())
        }
    })();
    match result {
        Ok(()) => {
            window.set_tools_error(SharedString::default());
            window.set_tools_working(true);
        }
        Err(error) => window.set_tools_error(SharedString::from(error)),
    }
}

fn cancel_active_tool(state: &Rc<RefCell<AppState>>, window: &AppWindow) {
    let mut app = state.borrow_mut();
    let Some(active) = app.active_tool_job.as_ref() else {
        return;
    };
    if let Some(worker) = app.tool_worker.as_ref() {
        worker.cancel(active.key, &active.cancellation);
    }
    clear_tool_password_ui(window);
    window.set_tool_password_prompt_open(false);
    app.tool_password_source = None;
    window.set_tools_working(false);
    app.wake_pump();
}

pub(crate) fn handle_tool_event(
    event: ToolEvent,
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
) {
    let key = event.key();
    let current = {
        let app = state.borrow();
        app.active_tool_job.as_ref().is_some_and(|active| {
            active.key == key && key.is_current(app.generation, app.tool_source_token)
        })
    };
    if !current {
        if event.is_terminal() {
            let mut app = state.borrow_mut();
            if app
                .active_tool_job
                .as_ref()
                .is_some_and(|active| active.key == key)
            {
                app.active_tool_job = None;
                window.set_tools_working(false);
                clear_tool_password_ui(window);
                window.set_tool_password_prompt_open(false);
            }
        }
        return;
    }
    match event {
        ToolEvent::PasswordRequired {
            source,
            wrong_password,
            ..
        } => {
            let error = barepdf_i18n::t(
                window_language(window),
                if wrong_password {
                    "tools.password.incorrect"
                } else {
                    "tools.password.required"
                },
            );
            state.borrow_mut().tool_password_source = Some(source);
            clear_tool_password_ui(window);
            window.set_tool_password_error(SharedString::from(error));
            window.set_tool_password_prompt_open(true);
        }
        ToolEvent::Completed { outcome, .. } => {
            state.borrow_mut().active_tool_job = None;
            state.borrow_mut().tool_password_source = None;
            window.set_tools_working(false);
            clear_tool_password_ui(window);
            window.set_tool_password_prompt_open(false);
            window.set_tools_open(false);
            window.set_current_tool(-1);
            refresh_thumbnail_model(&mut state.borrow_mut(), window);
            match outcome {
                ToolOutcome::Pdf { output } => {
                    show_banner(
                        window,
                        barepdf_i18n::t(window_language(window), "tools.status.success"),
                        false,
                    );
                    begin_open(output, None, state, scheduler, window);
                }
                ToolOutcome::Split {
                    output_directory,
                    file_count,
                } => show_banner(
                    window,
                    format!(
                        "Created {file_count} PDF files in {}.",
                        output_directory.display()
                    ),
                    false,
                ),
                ToolOutcome::Conversion(report) => show_banner(
                    window,
                    format!(
                        "Converted {} file(s) in {}.",
                        report.files.len(),
                        report.output_directory.display()
                    ),
                    false,
                ),
                #[cfg(test)]
                ToolOutcome::Test => {}
            }
        }
        ToolEvent::Cancelled { .. } => {
            state.borrow_mut().active_tool_job = None;
            state.borrow_mut().tool_password_source = None;
            window.set_tools_working(false);
            clear_tool_password_ui(window);
            window.set_tool_password_prompt_open(false);
        }
        ToolEvent::Failed { message, .. } => {
            state.borrow_mut().active_tool_job = None;
            state.borrow_mut().tool_password_source = None;
            window.set_tools_working(false);
            clear_tool_password_ui(window);
            window.set_tool_password_prompt_open(false);
            window.set_tools_error(SharedString::from(message));
        }
    }
}

pub(super) fn connect_tools_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    _scheduler: &Rc<RenderScheduler>,
    dialogs: Arc<WindowsFileDialogs>,
) {
    window.on_request_open_url(move |url| {
        let _ = open_url(url.as_str());
    });

    let weak = window.as_weak();
    let state_toggle_tools = state.clone();
    window.on_request_toggle_tools(move || {
        if let Some(window) = weak.upgrade() {
            let next_open = !window.get_tools_open();
            if !next_open {
                cancel_active_tool(&state_toggle_tools, &window);
                refresh_thumbnail_model(&mut state_toggle_tools.borrow_mut(), &window);
            }
            window.set_tools_open(next_open);
            if next_open {
                window.set_current_tool(-1);
                window.set_tools_error(SharedString::new());
                window.set_tools_working(false);
            }
        }
    });

    let weak = window.as_weak();
    let state_open_tool = state.clone();
    window.on_request_open_tool(move |tool_id| {
        if let Some(window) = weak.upgrade() {
            {
                let mut app = state_open_tool.borrow_mut();
                if app.active_tool_job.is_none() {
                    app.tools_source_path = None;
                    app.tool_source_token = app.tool_source_token.wrapping_add(1).max(1);
                }
            }
            window.set_current_tool(tool_id);
            window.set_tools_error(SharedString::new());
            window.set_tools_working(false);

            if tool_id == 0 {
                let mut app = state_open_tool.borrow_mut();
                refresh_merge_files(&window, &mut app);
                window.set_selected_merge_index(-1);
            } else if tool_id == 1 || tool_id == 2 || tool_id == 3 {
                let mut app = state_open_tool.borrow_mut();
                let has_doc = app.application.ready_document().is_some();
                let range = if has_doc {
                    let page_num = app.current_page + 1;
                    page_num.to_string()
                } else {
                    String::new()
                };
                window.set_tools_page_range(SharedString::from(range.clone()));
                window.set_tools_split_mode(0);
                window.set_tools_rotation(1);
                refresh_tool_thumbnails(&window, &mut app, &range);
            } else if tool_id == 4 {
                let mut app = state_open_tool.borrow_mut();
                let count = app.page_count();
                let range = if count > 0 {
                    if count == 1 {
                        "1".to_string()
                    } else {
                        format!("1-{}", count)
                    }
                } else {
                    String::new()
                };
                window.set_tools_page_range(SharedString::from(range.clone()));
                refresh_tool_thumbnails(&window, &mut app, &range);
            } else if tool_id == -1 {
                let mut app = state_open_tool.borrow_mut();
                refresh_thumbnail_model(&mut app, &window);
            }
        }
    });

    let weak = window.as_weak();
    let state_close_tools = state.clone();
    window.on_request_close_tools(move || {
        if let Some(window) = weak.upgrade() {
            cancel_active_tool(&state_close_tools, &window);
            window.set_tools_open(false);
            window.set_current_tool(-1);
            window.set_tools_error(SharedString::new());
            window.set_tools_working(false);
            refresh_thumbnail_model(&mut state_close_tools.borrow_mut(), &window);
        }
    });

    let weak = window.as_weak();
    let state_merge_add = state.clone();
    let dialogs_merge_add = dialogs.clone();
    window.on_request_merge_add_files(move || {
        let picked = dialogs_merge_add.pick_multiple_files();
        if !picked.is_empty() {
            let mut app = state_merge_add.borrow_mut();
            app.tools_merge_files.extend(picked);
            if let Some(window) = weak.upgrade() {
                refresh_merge_files(&window, &mut app);
                window.set_tools_error(SharedString::new());
            }
        }
    });

    let weak = window.as_weak();
    window.on_request_merge_select_file(move |idx| {
        if let Some(window) = weak.upgrade() {
            window.set_selected_merge_index(idx);
        }
    });

    let weak = window.as_weak();
    let state_move_up = state.clone();
    window.on_request_merge_move_up(move |idx| {
        if idx > 0 {
            let idx = idx as usize;
            let mut app = state_move_up.borrow_mut();
            if idx < app.tools_merge_files.len() {
                app.tools_merge_files.swap(idx, idx - 1);
                if let Some(window) = weak.upgrade() {
                    refresh_merge_files(&window, &mut app);
                    window.set_selected_merge_index((idx - 1) as i32);
                }
            }
        }
    });

    let weak = window.as_weak();
    let state_move_down = state.clone();
    window.on_request_merge_move_down(move |idx| {
        if idx >= 0 {
            let idx = idx as usize;
            let mut app = state_move_down.borrow_mut();
            if idx + 1 < app.tools_merge_files.len() {
                app.tools_merge_files.swap(idx, idx + 1);
                if let Some(window) = weak.upgrade() {
                    refresh_merge_files(&window, &mut app);
                    window.set_selected_merge_index((idx + 1) as i32);
                }
            }
        }
    });

    let weak = window.as_weak();
    let state_remove = state.clone();
    window.on_request_merge_remove_file(move |idx| {
        if idx >= 0 {
            let idx = idx as usize;
            let mut app = state_remove.borrow_mut();
            if idx < app.tools_merge_files.len() {
                app.tools_merge_files.remove(idx);
                let new_selected = if app.tools_merge_files.is_empty() {
                    -1
                } else if idx >= app.tools_merge_files.len() {
                    (app.tools_merge_files.len() - 1) as i32
                } else {
                    idx as i32
                };
                if let Some(window) = weak.upgrade() {
                    refresh_merge_files(&window, &mut app);
                    window.set_selected_merge_index(new_selected);
                }
            }
        }
    });

    let weak = window.as_weak();
    let state_clear = state.clone();
    window.on_request_merge_clear(move || {
        let mut app = state_clear.borrow_mut();
        app.tools_merge_files.clear();
        if let Some(window) = weak.upgrade() {
            refresh_merge_files(&window, &mut app);
            window.set_selected_merge_index(-1);
        }
    });

    let weak = window.as_weak();
    let state_merge_exec = state.clone();
    let dialogs_merge_exec = dialogs.clone();
    window.on_request_merge_execute(move || {
        let files = state_merge_exec.borrow().tools_merge_files.clone();
        let language = state_merge_exec.borrow().preferences.language.resolve();
        if files.len() < 2 {
            if let Some(window) = weak.upgrade() {
                window.set_tools_error(SharedString::from(barepdf_i18n::t(
                    language,
                    "tools.error.no_files",
                )));
            }
            return;
        }

        let Some(output_path) = dialogs_merge_exec.save_file("merged.pdf") else {
            return;
        };

        if let Some(window) = weak.upgrade() {
            queue_tool_operation(
                ToolOperation::Merge {
                    inputs: files,
                    output: output_path,
                },
                &state_merge_exec,
                &window,
            );
        }
    });

    let weak = window.as_weak();
    let state_split_exec = state.clone();
    let dialogs_split_exec = dialogs.clone();
    window.on_request_split_execute(move |range_str, mode| {
        let source_path = {
            let app = state_split_exec.borrow();
            let lang = app.preferences.language.resolve();
            let Some(source) = current_tool_source(&app) else {
                if let Some(window) = weak.upgrade() {
                    window.set_tools_error(SharedString::from(barepdf_i18n::t(
                        lang,
                        "tools.error.no_files",
                    )));
                }
                return;
            };
            let _ = lang;
            source
        };

        if mode == 0 {
            let default_name = format!(
                "{}_extracted.pdf",
                source_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "document".to_string())
            );
            let Some(output_path) = dialogs_split_exec.save_file(&default_name) else {
                return;
            };

            if let Some(window) = weak.upgrade() {
                set_tool_source(&mut state_split_exec.borrow_mut(), source_path.clone());
                queue_tool_operation(
                    ToolOperation::Extract {
                        source: source_path,
                        range: range_str.to_string(),
                        output: output_path,
                    },
                    &state_split_exec,
                    &window,
                );
            }
        } else {
            let Some(output_dir) = dialogs_split_exec.pick_directory() else {
                return;
            };

            let base_name = source_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "document".to_string());

            if let Some(window) = weak.upgrade() {
                set_tool_source(&mut state_split_exec.borrow_mut(), source_path.clone());
                queue_tool_operation(
                    ToolOperation::SplitAll {
                        source: source_path,
                        output_parent: output_dir,
                        base_name,
                    },
                    &state_split_exec,
                    &window,
                );
            }
        }
    });

    let weak = window.as_weak();
    let state_del_exec = state.clone();
    let dialogs_del_exec = dialogs.clone();
    window.on_request_delete_pages_execute(move |range_str| {
        let source_path = {
            let app = state_del_exec.borrow();
            let lang = app.preferences.language.resolve();
            let Some(source) = current_tool_source(&app) else {
                if let Some(window) = weak.upgrade() {
                    window.set_tools_error(SharedString::from(barepdf_i18n::t(
                        lang,
                        "tools.error.no_files",
                    )));
                }
                return;
            };
            source
        };

        let default_name = format!(
            "{}_modified.pdf",
            source_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "document".to_string())
        );
        let Some(output_path) = dialogs_del_exec.save_file(&default_name) else {
            return;
        };

        if let Some(window) = weak.upgrade() {
            set_tool_source(&mut state_del_exec.borrow_mut(), source_path.clone());
            queue_tool_operation(
                ToolOperation::Delete {
                    source: source_path,
                    range: range_str.to_string(),
                    output: output_path,
                },
                &state_del_exec,
                &window,
            );
        }
    });

    let weak = window.as_weak();
    let state_rot_exec = state.clone();
    let dialogs_rot_exec = dialogs.clone();
    window.on_request_rotate_pages_execute(move |range_str, rot_val| {
        let source_path = {
            let app = state_rot_exec.borrow();
            let lang = app.preferences.language.resolve();
            let Some(source) = current_tool_source(&app) else {
                if let Some(window) = weak.upgrade() {
                    window.set_tools_error(SharedString::from(barepdf_i18n::t(
                        lang,
                        "tools.error.no_files",
                    )));
                }
                return;
            };
            source
        };

        let target_rot = match rot_val {
            1 => Rotation::Degrees90,
            2 => Rotation::Degrees180,
            3 => Rotation::Degrees270,
            _ => Rotation::Degrees90,
        };

        let default_name = format!(
            "{}_rotated.pdf",
            source_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "document".to_string())
        );
        let Some(output_path) = dialogs_rot_exec.save_file(&default_name) else {
            return;
        };

        if let Some(window) = weak.upgrade() {
            set_tool_source(&mut state_rot_exec.borrow_mut(), source_path.clone());
            queue_tool_operation(
                ToolOperation::Rotate {
                    source: source_path,
                    range: range_str.to_string(),
                    rotation: target_rot,
                    output: output_path,
                },
                &state_rot_exec,
                &window,
            );
        }
    });

    let weak = window.as_weak();
    let state_crop_exec = state.clone();
    let dialogs_crop_exec = dialogs.clone();
    window.on_request_crop_pages_execute(
        move |range_str, left_str, bottom_str, right_str, top_str| {
            let (source_path, page_count) = {
                let app = state_crop_exec.borrow();
                let lang = app.preferences.language.resolve();
                let Some(source) = current_tool_source(&app) else {
                    if let Some(window) = weak.upgrade() {
                        window.set_tools_error(SharedString::from(barepdf_i18n::t(
                            lang,
                            "tools.error.no_files",
                        )));
                    }
                    return;
                };
                (source, app.page_count())
            };

            let left = left_str.trim().parse::<f32>().unwrap_or(36.0);
            let bottom = bottom_str.trim().parse::<f32>().unwrap_or(36.0);
            let right = right_str.trim().parse::<f32>().unwrap_or(576.0);
            let top = top_str.trim().parse::<f32>().unwrap_or(756.0);

            let pages = selected_tool_pages(&range_str, page_count);
            let target_pages: Vec<u32> = if pages.is_empty() {
                (1..=page_count).collect()
            } else {
                pages
            };

            let crops: Vec<barepdf_core::PageCropRect> = target_pages
                .into_iter()
                .map(|p| barepdf_core::PageCropRect {
                    page_index: p.saturating_sub(1) as usize,
                    left,
                    bottom,
                    right,
                    top,
                })
                .collect();

            let default_name = format!(
                "{}_cropped.pdf",
                source_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "document".to_string())
            );
            let Some(output_path) = dialogs_crop_exec.save_file(&default_name) else {
                return;
            };

            if let Some(window) = weak.upgrade() {
                set_tool_source(&mut state_crop_exec.borrow_mut(), source_path.clone());
                queue_tool_operation(
                    ToolOperation::Crop {
                        source: source_path,
                        crops,
                        output: output_path,
                    },
                    &state_crop_exec,
                    &window,
                );
            }
        },
    );

    let weak = window.as_weak();
    let state_reorder_exec = state.clone();
    let dialogs_reorder_exec = dialogs.clone();
    window.on_request_reorder_pages_execute(move |order_str| {
        let (source_path, page_count) = {
            let app = state_reorder_exec.borrow();
            let lang = app.preferences.language.resolve();
            let Some(source) = current_tool_source(&app) else {
                if let Some(window) = weak.upgrade() {
                    window.set_tools_error(SharedString::from(barepdf_i18n::t(
                        lang,
                        "tools.error.no_files",
                    )));
                }
                return;
            };
            (source, app.page_count())
        };

        let trimmed = order_str.trim();
        let parsed_indices: Result<Vec<u32>, _> = trimmed
            .split([',', ' '])
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().parse::<u32>())
            .collect();

        let Ok(indices) = parsed_indices else {
            if let Some(window) = weak.upgrade() {
                window.set_tools_error(SharedString::from(
                    "Invalid page order. Enter comma-separated page numbers.",
                ));
            }
            return;
        };

        if indices.len() != page_count as usize {
            if let Some(window) = weak.upgrade() {
                window.set_tools_error(SharedString::from(format!(
                    "Order must contain exactly {} pages (found {}).",
                    page_count,
                    indices.len()
                )));
            }
            return;
        }

        let mut seen = std::collections::HashSet::new();
        let mut new_order = Vec::with_capacity(indices.len());
        for &idx in &indices {
            if idx < 1 || idx > page_count || !seen.insert(idx) {
                if let Some(window) = weak.upgrade() {
                    window.set_tools_error(SharedString::from(
                        "Order contains invalid or duplicate page numbers.",
                    ));
                }
                return;
            }
            new_order.push(barepdf_core::PageIndex::from_raw(idx - 1));
        }

        let default_name = format!(
            "{}_reordered.pdf",
            source_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "document".to_string())
        );
        let Some(output_path) = dialogs_reorder_exec.save_file(&default_name) else {
            return;
        };

        if let Some(window) = weak.upgrade() {
            set_tool_source(&mut state_reorder_exec.borrow_mut(), source_path.clone());
            queue_tool_operation(
                ToolOperation::Reorder {
                    source: source_path,
                    new_order,
                    output: output_path,
                },
                &state_reorder_exec,
                &window,
            );
        }
    });

    let weak = window.as_weak();
    let state_reorder = state.clone();
    window.on_request_merge_reorder(move |from, to| {
        if from < 0 || to < 0 {
            return;
        }
        let mut app = state_reorder.borrow_mut();
        let (from, to) = (from as usize, to as usize);
        if from >= app.tools_merge_files.len() || to >= app.tools_merge_files.len() || from == to {
            return;
        }
        let moved = app.tools_merge_files.remove(from);
        app.tools_merge_files.insert(to, moved);
        if let Some(window) = weak.upgrade() {
            refresh_merge_files(&window, &mut app);
            window.set_selected_merge_index(to as i32);
        }
    });

    let weak = window.as_weak();
    let state_drop = state.clone();
    window.on_request_tool_drop(move |transfer, tool_id| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let paths = transfer
            .plain_text()
            .ok()
            .map_or_else(Vec::new, |text| tool_drop_paths(text.as_str()));
        if paths.is_empty() {
            window.set_tools_error(SharedString::from(barepdf_i18n::t(
                window_language(&window),
                "tools.error.drop_pdf",
            )));
            return;
        }
        let mut app = state_drop.borrow_mut();
        if app.active_tool_job.is_some() {
            window.set_tools_error(SharedString::from(barepdf_i18n::t(
                window_language(&window),
                "tools.error.busy",
            )));
            return;
        }
        match ToolKind::from_slint_id(tool_id) {
            ToolKind::Merge => {
                app.tools_merge_files.extend(paths);
                refresh_merge_files(&window, &mut app);
                window.set_tools_error(SharedString::default());
            }
            ToolKind::SingleSource => {
                if let [source] = paths.as_slice() {
                    set_tool_source(&mut app, source.clone());
                    window.set_tools_error(SharedString::default());
                } else {
                    window.set_tools_error(SharedString::from(barepdf_i18n::t(
                        window_language(&window),
                        "tools.error.single_source",
                    )));
                }
            }
        }
    });

    let weak = window.as_weak();
    let state_selection = state.clone();
    window.on_request_select_page_range(move |page, _ctrl, shift| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_selection.borrow_mut();
        let page = u32::try_from(page).unwrap_or(0).saturating_add(1);
        let total = app.page_count();
        if page == 0 || page > total {
            return;
        }
        let existing = window.get_tools_page_range().to_string();
        let range = toggle_tool_page_selection(&existing, page, total, shift);
        window.set_tools_page_range(SharedString::from(range.clone()));
        refresh_tool_thumbnails(&window, &mut app, &range);
        app.wake_pump();
    });

    let weak = window.as_weak();
    let state_select_all = state.clone();
    window.on_request_select_all_tool_pages(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_select_all.borrow_mut();
        let count = app.page_count();
        if count > 0 {
            let range = if count == 1 {
                "1".to_string()
            } else {
                format!("1-{}", count)
            };
            window.set_tools_page_range(SharedString::from(range.clone()));
            refresh_tool_thumbnails(&window, &mut app, &range);
            app.wake_pump();
        }
    });

    let weak = window.as_weak();
    let state_clear_pages = state.clone();
    window.on_request_clear_tool_pages(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let mut app = state_clear_pages.borrow_mut();
        window.set_tools_page_range(SharedString::default());
        refresh_tool_thumbnails(&window, &mut app, "");
        app.wake_pump();
    });

    let weak = window.as_weak();
    let state_convert = state.clone();
    let dialogs_convert = dialogs.clone();
    window.on_request_convert_execute(move |format, dpi, _quality, range| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let format = match format {
            0 => ConversionFormat::Text,
            1 => ConversionFormat::Markdown,
            2 => ConversionFormat::Png,
            3 => ConversionFormat::Jpeg,
            _ => {
                window.set_tools_error(SharedString::from(barepdf_i18n::t(
                    window_language(&window),
                    "tools.error.format",
                )));
                return;
            }
        };
        let dpi = match dpi {
            150 => ConversionDpi::Dpi150,
            300 => ConversionDpi::Dpi300,
            _ => {
                window.set_tools_error(SharedString::from(barepdf_i18n::t(
                    window_language(&window),
                    "tools.error.resolution",
                )));
                return;
            }
        };
        let source = {
            let app = state_convert.borrow();
            current_tool_source(&app)
        };
        let Some(source) = source else {
            window.set_tools_error(SharedString::from(barepdf_i18n::t(
                window_language(&window),
                "tools.error.source",
            )));
            return;
        };
        let Some(output_parent) = dialogs_convert.pick_directory() else {
            return;
        };
        set_tool_source(&mut state_convert.borrow_mut(), source.clone());
        queue_tool_operation(
            ToolOperation::Convert {
                source,
                output_parent,
                range: range.to_string(),
                format,
                dpi,
            },
            &state_convert,
            &window,
        );
    });

    let weak = window.as_weak();
    let state_password = state.clone();
    window.on_request_submit_tool_password(move |password| {
        let Ok(mut password) = consume_ui_password(password) else {
            if let Some(window) = weak.upgrade() {
                clear_tool_password_ui(&window);
                window.set_tool_password_error(SharedString::from(barepdf_i18n::t(
                    window_language(&window),
                    "password.error.too_long",
                )));
            }
            return;
        };
        let Some(window) = weak.upgrade() else {
            password.clear();
            return;
        };
        clear_tool_password_ui(&window);
        let result = {
            let app = state_password.borrow();
            let Some(active) = app.active_tool_job.as_ref() else {
                password.clear();
                return;
            };
            let Some(source) = app
                .tool_password_source
                .clone()
                .or_else(|| app.tools_source_path.clone())
            else {
                password.clear();
                return;
            };
            let Some(worker) = app.tool_worker.as_ref() else {
                password.clear();
                return;
            };
            worker.provide_password(active.key, source, password)
        };
        match result {
            Ok(()) => {
                window.set_tool_password_error(SharedString::default());
                window.set_tool_password_prompt_open(false);
                state_password.borrow_mut().wake_pump();
            }
            Err(error) => window.set_tool_password_error(SharedString::from(error.to_string())),
        }
    });

    let weak = window.as_weak();
    let state_cancel_password = state.clone();
    window.on_request_cancel_tool_password(move || {
        if let Some(window) = weak.upgrade() {
            clear_tool_password_ui(&window);
            window.set_tool_password_error(SharedString::default());
            cancel_active_tool(&state_cancel_password, &window);
        }
    });
}
