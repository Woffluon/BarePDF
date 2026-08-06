use crate::types::{PageIndex, ReadingDirection, ViewingMode, ZoomMode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AppCommand {
    OpenDocument,
    CloseDocument,
    PrintDocument,
    CopySelectedText,
    FindText,
    GoToPage(PageIndex),
    NextPage,
    PrevPage,
    FirstPage,
    LastPage,
    ZoomIn,
    ZoomOut,
    SetZoomMode(ZoomMode),
    SetViewingMode(ViewingMode),
    SetReadingDirection(ReadingDirection),
    ToggleSidebar,
    ToggleFullScreen,
    TogglePresentationMode,
    RotateCW,
}
