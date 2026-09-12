use crate::presentation::model::PaperTintColor;
use barepdf_core::{PageIndex, ViewingMode, ZoomMode};
use std::path::PathBuf;

/// Strongly typed message representing any user action or system event in TEA.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    // Tab and document operations
    OpenDocument(PathBuf),
    CloseTab(usize),
    SelectTab(usize),
    UnlockPasswordSubmitted(String),
    UnlockPasswordCancelled,

    // Navigation and viewing
    NextPage,
    PreviousPage,
    GoToPage(PageIndex),
    SetZoom(ZoomMode),
    ZoomIn,
    ZoomOut,
    RotateClockwise,
    SetViewingMode(ViewingMode),
    ScrollPositionChanged(f32),

    // Visual ergonomics and niche features
    ToggleZenMode,
    ToggleCommandPalette,
    CommandPaletteQueryChanged(String),
    ExecuteCommand(String),
    SetPaperTint(PaperTintColor),

    // Window and system
    WindowResized { width: u32, height: u32 },
}
