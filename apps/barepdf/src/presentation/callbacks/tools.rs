use crate::infrastructure::{
    ToolEvent, ToolJobKey, ToolOperation, ToolOutcome, ToolRequest, ToolWorker,
};
use barepdf_core::{page_range::PageRangeSelection, CompressionMode, PageCount, Rotation};
use barepdf_i18n::{t, ResolvedLanguage};
use barepdf_pdf::conversion::{ConversionDpi, ConversionFormat};
use barepdf_platform_windows::{open_url, WindowsFileDialogs};
use barepdf_render::{RenderKind, RenderScheduler};
use barepdf_ui::{AppWindow, ThumbnailItem};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
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
        .tools
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
        .tools
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
    app.tools.tools_source_path.clone().or_else(|| {
        app.application
            .ready_document()
            .map(|document| document.path().to_path_buf())
    })
}

pub(crate) fn set_tool_source(app: &mut AppState, source: PathBuf) {
    if app.tools.tools_source_path.as_ref() != Some(&source) {
        app.tools.tools_source_path = Some(source);
        app.tools.tool_source_token = app.tools.tool_source_token.wrapping_add(1).max(1);
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
    let id = app.tools.next_tool_job_id;
    app.tools.next_tool_job_id = app.tools.next_tool_job_id.wrapping_add(1).max(1);
    ToolJobKey::new(id, app.generation, app.tools.tool_source_token)
}

fn queue_tool_operation(
    operation: ToolOperation,
    state: &Rc<RefCell<AppState>>,
    window: &AppWindow,
) {
    let result = (|| -> Result<(), String> {
        let mut app = state.borrow_mut();
        if app.tools.active_tool_job.is_some() {
            Err(barepdf_i18n::t(app.preferences.language.resolve(), "tools.error.busy").to_owned())
        } else {
            if app.tools.tool_worker.is_none() {
                app.tools.tool_worker =
                    Some(ToolWorker::spawn().map_err(|error| error.to_string())?);
            }
            let key = next_tool_job_key(&mut app);
            let request = ToolRequest::new(key, operation);
            let Some(worker) = app.tools.tool_worker.as_ref() else {
                return Err("PDF tool worker is unavailable".to_owned());
            };
            let cancellation = worker.submit(request).map_err(|error| error.to_string())?;
            app.tools.active_tool_job = Some(ActiveToolJob { key, cancellation });
            app.tools.tool_password_source = None;
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
    let Some(active) = app.tools.active_tool_job.as_ref() else {
        return;
    };
    if let Some(worker) = app.tools.tool_worker.as_ref() {
        worker.cancel(active.key, &active.cancellation);
    }
    clear_tool_password_ui(window);
    window.set_tool_password_prompt_open(false);
    app.tools.tool_password_source = None;
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
        app.tools.active_tool_job.as_ref().is_some_and(|active| {
            active.key == key && key.is_current(app.generation, app.tools.tool_source_token)
        })
    };
    if !current {
        if event.is_terminal() {
            let mut app = state.borrow_mut();
            if app
                .tools
                .active_tool_job
                .as_ref()
                .is_some_and(|active| active.key == key)
            {
                app.tools.active_tool_job = None;
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
            state.borrow_mut().tools.tool_password_source = Some(source);
            clear_tool_password_ui(window);
            window.set_tool_password_error(SharedString::from(error));
            window.set_tool_password_prompt_open(true);
        }
        ToolEvent::Completed { outcome, .. } => {
            state.borrow_mut().tools.active_tool_job = None;
            state.borrow_mut().tools.tool_password_source = None;
            window.set_tools_working(false);
            clear_tool_password_ui(window);
            window.set_tool_password_prompt_open(false);
            refresh_thumbnail_model(&mut state.borrow_mut(), window);
            match outcome {
                ToolOutcome::Pdf { output } => {
                    window.set_tools_open(false);
                    window.set_current_tool(0);
                    show_banner(
                        window,
                        t(window_language(window), "tools.status.success"),
                        false,
                    );
                    begin_open(output, None, state, scheduler, window);
                }
                ToolOutcome::Split {
                    output_directory,
                    file_count,
                } => {
                    window.set_tools_open(false);
                    window.set_current_tool(0);
                    show_banner(
                        window,
                        format!(
                            "Created {file_count} PDF files in {}.",
                            output_directory.display()
                        ),
                        false,
                    );
                }
                ToolOutcome::Conversion(report) => {
                    window.set_tools_open(false);
                    window.set_current_tool(0);
                    show_banner(
                        window,
                        format!(
                            "Converted {} file(s) in {}.",
                            report.files.len(),
                            report.output_directory.display()
                        ),
                        false,
                    );
                }
                ToolOutcome::Compress { output, report } => {
                    let before_str = format_byte_size(report.original_bytes);
                    let after_str = format_byte_size(report.compressed_bytes);
                    let saved_str = format!("{:.1}%", report.saved_percent);
                    window.set_compress_has_result(true);
                    window.set_compress_unchanged(report.compressed_bytes >= report.original_bytes);
                    window.set_compress_before_size(SharedString::from(&before_str));
                    window.set_compress_after_size(SharedString::from(&after_str));
                    window.set_compress_saved_percent(SharedString::from(&saved_str));
                    window
                        .set_compress_output_path(SharedString::from(output.display().to_string()));
                    show_banner(
                        window,
                        format!("{before_str} -> {after_str} (-{saved_str})"),
                        false,
                    );
                }
                #[cfg(test)]
                ToolOutcome::Test => {
                    window.set_tools_open(false);
                    window.set_current_tool(0);
                }
            }
        }
        ToolEvent::Cancelled { .. } => {
            state.borrow_mut().tools.active_tool_job = None;
            state.borrow_mut().tools.tool_password_source = None;
            window.set_tools_working(false);
            clear_tool_password_ui(window);
            window.set_tool_password_prompt_open(false);
        }
        ToolEvent::Failed { message, .. } => {
            state.borrow_mut().tools.active_tool_job = None;
            state.borrow_mut().tools.tool_password_source = None;
            window.set_tools_working(false);
            clear_tool_password_ui(window);
            window.set_tool_password_prompt_open(false);
            window.set_tools_error(SharedString::from(message));
        }
    }
}

pub(crate) fn sync_tools_i18n(window: &AppWindow, language: ResolvedLanguage) {
    window.set_text_tools_split_extract_short(SharedString::from(t(
        language,
        "tools.split_mode.extract_short",
    )));
    window.set_text_tools_split_separate_short(SharedString::from(t(
        language,
        "tools.split_mode.separate_short",
    )));
    window.set_text_tools_rotation_90_short(SharedString::from(t(
        language,
        "tools.rotation.90_short",
    )));
    window.set_text_tools_rotation_180_short(SharedString::from(t(
        language,
        "tools.rotation.180_short",
    )));
    window.set_text_tools_rotation_270_short(SharedString::from(t(
        language,
        "tools.rotation.270_short",
    )));
    window.set_text_tools_drop_crop(SharedString::from(t(language, "tools.drop.crop")));
    window.set_text_tools_drop_reorder(SharedString::from(t(language, "tools.drop.reorder")));
    window.set_text_tools_drop_compress(SharedString::from(t(language, "tools.drop.compress")));
    window
        .set_text_tools_crop_placeholder(SharedString::from(t(language, "tools.crop.placeholder")));
    window.set_text_tools_crop_preset(SharedString::from(t(language, "tools.crop.preset")));
    window.set_text_tools_crop_preset_narrow(SharedString::from(t(
        language,
        "tools.crop.preset.narrow",
    )));
    window.set_text_tools_crop_preset_normal(SharedString::from(t(
        language,
        "tools.crop.preset.normal",
    )));
    window
        .set_text_tools_crop_preset_wide(SharedString::from(t(language, "tools.crop.preset.wide")));
    window.set_text_tools_crop_preset_custom(SharedString::from(t(
        language,
        "tools.crop.preset.custom",
    )));
    window.set_text_tools_crop_preview(SharedString::from(t(language, "tools.crop.preview")));
    window.set_text_tools_crop_left(SharedString::from(t(language, "tools.crop.left")));
    window.set_text_tools_crop_right(SharedString::from(t(language, "tools.crop.right")));
    window.set_text_tools_crop_top(SharedString::from(t(language, "tools.crop.top")));
    window.set_text_tools_crop_bottom(SharedString::from(t(language, "tools.crop.bottom")));
    window.set_text_tools_reorder_hint(SharedString::from(t(language, "tools.reorder.hint")));
    window.set_text_tools_reorder_move_earlier(SharedString::from(t(
        language,
        "tools.reorder.move_earlier",
    )));
    window.set_text_tools_reorder_move_later(SharedString::from(t(
        language,
        "tools.reorder.move_later",
    )));
    window.set_text_tools_reorder_reset(SharedString::from(t(language, "tools.reorder.reset")));
    window.set_text_tools_reorder_sequence_label(SharedString::from(t(
        language,
        "tools.reorder.sequence_label",
    )));
    window.set_text_tools_compress(SharedString::from(t(language, "tools.compress")));
    window.set_text_tools_compress_desc(SharedString::from(t(language, "tools.compress.desc")));
    window.set_text_tools_compress_mode(SharedString::from(t(language, "tools.compress.mode")));
    window.set_text_tools_compress_light(SharedString::from(t(
        language,
        "tools.compress.mode.light",
    )));
    window.set_text_tools_compress_balanced(SharedString::from(t(
        language,
        "tools.compress.mode.balanced",
    )));
    window.set_text_tools_compress_maximum(SharedString::from(t(
        language,
        "tools.compress.mode.maximum",
    )));
    window.set_text_tools_compress_light_detail(SharedString::from(t(
        language,
        "tools.compress.mode.light_detail",
    )));
    window.set_text_tools_compress_balanced_detail(SharedString::from(t(
        language,
        "tools.compress.mode.balanced_detail",
    )));
    window.set_text_tools_compress_maximum_detail(SharedString::from(t(
        language,
        "tools.compress.mode.maximum_detail",
    )));
    window.set_text_tools_compress_vector_note(SharedString::from(t(
        language,
        "tools.compress.vector_note",
    )));
    window
        .set_text_tools_compress_btn_run(SharedString::from(t(language, "tools.compress.btn_run")));
    window.set_text_tools_compress_result_title(SharedString::from(t(
        language,
        "tools.compress.result_title",
    )));
    window.set_text_tools_compress_result_unchanged(SharedString::from(t(
        language,
        "tools.compress.result_unchanged",
    )));
    window.set_text_tools_compress_before(SharedString::from(t(language, "tools.compress.before")));
    window.set_text_tools_compress_after(SharedString::from(t(language, "tools.compress.after")));
    window.set_text_tools_compress_saved(SharedString::from(t(language, "tools.compress.saved")));
    window.set_text_tools_compress_open_output(SharedString::from(t(
        language,
        "tools.compress.open_output",
    )));
}

pub(super) fn format_byte_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", b / KB)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.2} MB", b / MB)
    } else {
        format!("{:.2} GB", b / GB)
    }
}

pub(super) fn format_reorder_sequence(items: &[ThumbnailItem]) -> String {
    items
        .iter()
        .map(|item| (item.page_index + 1).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_crop_margin(raw: &str) -> Result<f32, &'static str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(0.0);
    }
    trimmed
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .ok_or("tools.error.invalid_range")
}

pub(super) fn compute_crop_boxes(
    range_str: &str,
    margins: (f32, f32, f32, f32),
    page_dimensions: &[(f32, f32)],
    fallback_dimensions: (f32, f32),
    total_pages: u32,
) -> Result<Vec<barepdf_core::PageCropRect>, &'static str> {
    let (left_margin, bottom_margin, right_margin, top_margin) = margins;
    if !left_margin.is_finite()
        || !bottom_margin.is_finite()
        || !right_margin.is_finite()
        || !top_margin.is_finite()
        || left_margin < 0.0
        || bottom_margin < 0.0
        || right_margin < 0.0
        || top_margin < 0.0
    {
        return Err("tools.error.invalid_range");
    }

    if total_pages == 0 {
        return Err("tools.error.no_pages");
    }

    let trimmed_range = range_str.trim();
    let target_pages = if trimmed_range.is_empty() {
        (1..=total_pages).collect()
    } else {
        let selected = selected_tool_pages(trimmed_range, total_pages);
        if selected.is_empty() {
            return Err("tools.error.invalid_range");
        }
        selected
    };

    let mut crops = Vec::with_capacity(target_pages.len());
    for page in target_pages {
        let page_index = page.saturating_sub(1) as usize;
        let (raw_w, raw_h) = page_dimensions
            .get(page_index)
            .copied()
            .unwrap_or(fallback_dimensions);
        let pw = if raw_w.is_finite() && raw_w > 1.0 {
            raw_w
        } else {
            612.0
        };
        let ph = if raw_h.is_finite() && raw_h > 1.0 {
            raw_h
        } else {
            792.0
        };

        let left = left_margin;
        let bottom = bottom_margin;
        let right = pw - right_margin;
        let top = ph - top_margin;
        if right <= left + 1.0 || top <= bottom + 1.0 {
            return Err("tools.error.invalid_range");
        }

        crops.push(barepdf_core::PageCropRect {
            page_index,
            left,
            bottom,
            right,
            top,
        });
    }

    Ok(crops)
}

fn apply_crop_preset_margins(window: &AppWindow, preset: i32) {
    let margin = match preset {
        0 => Some("18"),
        1 => Some("36"),
        2 => Some("54"),
        _ => None,
    };
    window.set_crop_preset(preset.clamp(0, 3));
    if let Some(val) = margin {
        let s = SharedString::from(val);
        window.set_crop_left_input(s.clone());
        window.set_crop_right_input(s.clone());
        window.set_crop_top_input(s.clone());
        window.set_crop_bottom_input(s);
    }
}

fn update_crop_preview_metrics(window: &AppWindow, app: &AppState, range: &str) {
    let total = app.page_count();
    let preview_idx = selected_tool_pages(range, total)
        .into_iter()
        .next()
        .map(|p| p.saturating_sub(1))
        .unwrap_or_else(|| {
            if total > 0 {
                app.current_page.min(total - 1)
            } else {
                0
            }
        });
    let (raw_w, raw_h) = app
        .page_dimensions
        .get(preview_idx as usize)
        .copied()
        .unwrap_or(app.first_page_dimensions);
    let pw = if raw_w.is_finite() && raw_w > 1.0 {
        raw_w
    } else {
        612.0
    };
    let ph = if raw_h.is_finite() && raw_h > 1.0 {
        raw_h
    } else {
        792.0
    };
    window.set_crop_preview_page_index(preview_idx as i32);
    window.set_crop_page_width(pw);
    window.set_crop_page_height(ph);
}

fn reset_reorder_model(window: &AppWindow, app: &mut AppState) {
    refresh_tool_thumbnails(window, app, "");
    let thumb_model = window.get_thumbnail_items();
    let mut items = Vec::with_capacity(thumb_model.row_count());
    for idx in 0..thumb_model.row_count() {
        if let Some(mut item) = thumb_model.row_data(idx) {
            item.is_selected = false;
            items.push(item);
        }
    }
    let sequence = format_reorder_sequence(&items);
    let initial_sel = if items.is_empty() { -1 } else { 0 };
    window.set_reorder_items(ModelRc::new(VecModel::from(items)));
    window.set_reorder_sequence_input(SharedString::from(sequence));
    window.set_selected_reorder_index(initial_sel);
    window.set_reorder_drag_index(-1);
    window.set_reorder_hover_index(-1);
}

fn refresh_compress_source_size(window: &AppWindow, app: &AppState) {
    let size_text = current_tool_source(app)
        .and_then(|path| std::fs::metadata(path).ok())
        .map(|meta| format_byte_size(meta.len()))
        .unwrap_or_default();
    window.set_compress_source_size(SharedString::from(size_text));
}

fn initialize_tool_panel(window: &AppWindow, app: &mut AppState, tool_id: i32) {
    window.set_current_tool(tool_id);
    window.set_tools_error(SharedString::new());
    window.set_tools_working(false);
    refresh_compress_source_size(window, app);

    match tool_id {
        0 => {
            refresh_merge_files(window, app);
            window.set_selected_merge_index(-1);
            window.set_merge_drag_index(-1);
            window.set_merge_hover_index(-1);
        }
        1..=3 => {
            let range = if app.application.ready_document().is_some() {
                (app.current_page + 1).to_string()
            } else {
                String::new()
            };
            window.set_tools_page_range(SharedString::from(range.clone()));
            window.set_tools_split_mode(0);
            window.set_tools_rotation(1);
            refresh_tool_thumbnails(window, app, &range);
        }
        4 => {
            let count = app.page_count();
            let range = if count == 0 {
                String::new()
            } else if count == 1 {
                "1".to_string()
            } else {
                format!("1-{count}")
            };
            window.set_tools_convert_scope(0);
            window.set_tools_page_range(SharedString::from(range.clone()));
            refresh_tool_thumbnails(window, app, &range);
        }
        5 => {
            window.set_tools_page_range(SharedString::new());
            apply_crop_preset_margins(window, window.get_crop_preset());
            refresh_tool_thumbnails(window, app, "");
            update_crop_preview_metrics(window, app, "");
        }
        6 => {
            reset_reorder_model(window, app);
        }
        7 => {
            window.set_compress_has_result(false);
            window.set_compress_unchanged(false);
            window.set_compress_before_size(SharedString::new());
            window.set_compress_after_size(SharedString::new());
            window.set_compress_saved_percent(SharedString::new());
            window.set_compress_output_path(SharedString::new());
        }
        _ => {}
    }
}

pub(super) fn connect_tools_callbacks(
    window: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    scheduler: &Rc<RenderScheduler>,
    dialogs: Arc<WindowsFileDialogs>,
) {
    sync_tools_i18n(window, window_language(window));

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
                let mut app = state_toggle_tools.borrow_mut();
                initialize_tool_panel(&window, &mut app, 0);
            }
        }
    });

    let weak = window.as_weak();
    let state_open_tool = state.clone();
    window.on_request_open_tool(move |tool_id| {
        if let Some(window) = weak.upgrade() {
            let mut app = state_open_tool.borrow_mut();
            if app.tools.active_tool_job.is_none() {
                app.tools.tools_source_path = None;
                app.tools.tool_source_token = app.tools.tool_source_token.wrapping_add(1).max(1);
            }
            let clamped = tool_id.clamp(0, 7);
            initialize_tool_panel(&window, &mut app, clamped);
        }
    });

    let weak = window.as_weak();
    let state_close_tools = state.clone();
    window.on_request_close_tools(move || {
        if let Some(window) = weak.upgrade() {
            cancel_active_tool(&state_close_tools, &window);
            window.set_tools_open(false);
            window.set_current_tool(0);
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
            app.tools.tools_merge_files.extend(picked);
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
            if idx < app.tools.tools_merge_files.len() {
                app.tools.tools_merge_files.swap(idx, idx - 1);
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
            if idx + 1 < app.tools.tools_merge_files.len() {
                app.tools.tools_merge_files.swap(idx, idx + 1);
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
            if idx < app.tools.tools_merge_files.len() {
                app.tools.tools_merge_files.remove(idx);
                let new_selected = if app.tools.tools_merge_files.is_empty() {
                    -1
                } else if idx >= app.tools.tools_merge_files.len() {
                    (app.tools.tools_merge_files.len() - 1) as i32
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
        app.tools.tools_merge_files.clear();
        if let Some(window) = weak.upgrade() {
            refresh_merge_files(&window, &mut app);
            window.set_selected_merge_index(-1);
        }
    });

    let weak = window.as_weak();
    let state_merge_exec = state.clone();
    let dialogs_merge_exec = dialogs.clone();
    window.on_request_merge_execute(move || {
        let files = state_merge_exec.borrow().tools.tools_merge_files.clone();
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
    window.on_request_crop_preset(move |preset| {
        if let Some(window) = weak.upgrade() {
            apply_crop_preset_margins(&window, preset);
            window.set_tools_error(SharedString::new());
        }
    });

    let weak = window.as_weak();
    let state_crop_exec = state.clone();
    let dialogs_crop_exec = dialogs.clone();
    window.on_request_crop_pages_execute(
        move |range_str, left_str, bottom_str, right_str, top_str| {
            let (source_path, page_count, page_dimensions, fallback_dimensions, lang) = {
                let app = state_crop_exec.borrow();
                let lang = app.preferences.language.resolve();
                let Some(source) = current_tool_source(&app) else {
                    if let Some(window) = weak.upgrade() {
                        window.set_tools_error(SharedString::from(t(lang, "tools.error.no_files")));
                    }
                    return;
                };
                (
                    source,
                    app.page_count(),
                    app.page_dimensions.clone(),
                    app.first_page_dimensions,
                    lang,
                )
            };

            let margins = (
                parse_crop_margin(&left_str),
                parse_crop_margin(&bottom_str),
                parse_crop_margin(&right_str),
                parse_crop_margin(&top_str),
            );
            let Ok((ml, mb, mr, mt)) = (match margins {
                (Ok(l), Ok(b), Ok(r), Ok(t)) => Ok((l, b, r, t)),
                (Err(e), _, _, _) | (_, Err(e), _, _) | (_, _, Err(e), _) | (_, _, _, Err(e)) => {
                    Err(e)
                }
            }) else {
                if let Some(window) = weak.upgrade() {
                    window
                        .set_tools_error(SharedString::from(t(lang, "tools.error.invalid_range")));
                }
                return;
            };

            let crops = match compute_crop_boxes(
                &range_str,
                (ml, mb, mr, mt),
                &page_dimensions,
                fallback_dimensions,
                page_count,
            ) {
                Ok(crops) => crops,
                Err(error_key) => {
                    if let Some(window) = weak.upgrade() {
                        window.set_tools_error(SharedString::from(t(lang, error_key)));
                    }
                    return;
                }
            };

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
    window.on_request_reorder_move(move |from, to| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        if from < 0 || to < 0 || from == to {
            return;
        }
        let model = window.get_reorder_items();
        let len = model.row_count();
        let (from_idx, to_idx) = (from as usize, to as usize);
        if from_idx >= len || to_idx >= len {
            return;
        }
        let mut items = Vec::with_capacity(len);
        for idx in 0..len {
            if let Some(item) = model.row_data(idx) {
                items.push(item);
            }
        }
        if items.len() != len {
            return;
        }
        let moved = items.remove(from_idx);
        items.insert(to_idx, moved);
        let sequence = format_reorder_sequence(&items);
        window.set_reorder_items(ModelRc::new(VecModel::from(items)));
        window.set_selected_reorder_index(to_idx as i32);
        window.set_reorder_sequence_input(SharedString::from(sequence));
        window.set_tools_error(SharedString::new());
    });

    let weak = window.as_weak();
    let state_reorder_reset = state.clone();
    window.on_request_reorder_reset(move || {
        if let Some(window) = weak.upgrade() {
            reset_reorder_model(&window, &mut state_reorder_reset.borrow_mut());
            window.set_tools_error(SharedString::new());
        }
    });

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
        if from >= app.tools.tools_merge_files.len()
            || to >= app.tools.tools_merge_files.len()
            || from == to
        {
            return;
        }
        let moved = app.tools.tools_merge_files.remove(from);
        app.tools.tools_merge_files.insert(to, moved);
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
        if app.tools.active_tool_job.is_some() {
            window.set_tools_error(SharedString::from(barepdf_i18n::t(
                window_language(&window),
                "tools.error.busy",
            )));
            return;
        }
        match ToolKind::from_slint_id(tool_id) {
            ToolKind::Merge => {
                app.tools.tools_merge_files.extend(paths);
                refresh_merge_files(&window, &mut app);
                window.set_tools_error(SharedString::default());
            }
            ToolKind::SingleSource => {
                if let [source] = paths.as_slice() {
                    set_tool_source(&mut app, source.clone());
                    refresh_compress_source_size(&window, &app);
                    window.set_compress_has_result(false);
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
        if window.get_current_tool() == 5 {
            update_crop_preview_metrics(&window, &app, &range);
        }
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
    let state_compress_exec = state.clone();
    let dialogs_compress_exec = dialogs.clone();
    window.on_request_compress_execute(move |mode_idx| {
        let (source_path, lang) = {
            let app = state_compress_exec.borrow();
            let lang = app.preferences.language.resolve();
            let Some(source) = current_tool_source(&app) else {
                if let Some(window) = weak.upgrade() {
                    window.set_tools_error(SharedString::from(t(lang, "tools.error.no_files")));
                }
                return;
            };
            (source, lang)
        };
        let _ = lang;

        let mode = CompressionMode::from_slint_index(mode_idx).unwrap_or(CompressionMode::Balanced);
        let default_name = format!(
            "{}_compressed.pdf",
            source_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "document".to_string())
        );
        let Some(output_path) = dialogs_compress_exec.save_file(&default_name) else {
            return;
        };

        if let Some(window) = weak.upgrade() {
            window.set_compress_has_result(false);
            window.set_compress_unchanged(false);
            set_tool_source(&mut state_compress_exec.borrow_mut(), source_path.clone());
            queue_tool_operation(
                ToolOperation::Compress {
                    source: source_path,
                    mode,
                    output: output_path,
                },
                &state_compress_exec,
                &window,
            );
        }
    });

    let weak = window.as_weak();
    let state_open_compressed = state.clone();
    let scheduler_open_compressed = scheduler.clone();
    window.on_request_open_compressed_file(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let raw_path = window.get_compress_output_path().to_string();
        if raw_path.is_empty() {
            return;
        }
        let path = PathBuf::from(raw_path);
        if !path.is_file() {
            return;
        }
        window.set_tools_open(false);
        window.set_current_tool(0);
        begin_open(
            path,
            None,
            &state_open_compressed,
            &scheduler_open_compressed,
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
            let Some(active) = app.tools.active_tool_job.as_ref() else {
                password.clear();
                return;
            };
            let Some(source) = app
                .tools
                .tool_password_source
                .clone()
                .or_else(|| app.tools.tools_source_path.clone())
            else {
                password.clear();
                return;
            };
            let Some(worker) = app.tools.tool_worker.as_ref() else {
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

#[cfg(test)]
mod tests {
    use super::{compute_crop_boxes, format_byte_size, format_reorder_sequence};
    use barepdf_ui::ThumbnailItem;

    #[test]
    fn format_byte_size_formats_bytes_kb_and_mb() {
        assert_eq!(format_byte_size(512), "512 B");
        assert_eq!(format_byte_size(1_536), "1.5 KB");
        assert_eq!(format_byte_size(2 * 1024 * 1024), "2.00 MB");
    }

    #[test]
    fn compute_crop_boxes_converts_margins_against_page_dimensions() {
        let dims = vec![(612.0, 792.0), (595.0, 842.0)];
        let crops = compute_crop_boxes("", (18.0, 36.0, 18.0, 36.0), &dims, (612.0, 792.0), 2)
            .expect("valid margins produce crop boxes");
        assert_eq!(crops.len(), 2);
        assert_eq!(crops[0].left, 18.0);
        assert_eq!(crops[0].bottom, 36.0);
        assert_eq!(crops[0].right, 594.0);
        assert_eq!(crops[0].top, 756.0);
        assert_eq!(crops[1].right, 577.0);
        assert_eq!(crops[1].top, 806.0);

        assert!(
            compute_crop_boxes("1", (400.0, 36.0, 300.0, 36.0), &dims, (612.0, 792.0), 2).is_err()
        );
    }

    #[test]
    fn format_reorder_sequence_builds_one_based_sequence() {
        let items = vec![
            ThumbnailItem {
                page_index: 2,
                ..Default::default()
            },
            ThumbnailItem {
                page_index: 0,
                ..Default::default()
            },
            ThumbnailItem {
                page_index: 1,
                ..Default::default()
            },
        ];
        assert_eq!(format_reorder_sequence(&items), "3, 1, 2");
    }
}
