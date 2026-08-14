use crate::infrastructure::VerifiedUpdate;
use std::path::PathBuf;

#[derive(Clone, Copy)]
pub(crate) enum UpdateUiState {
    Ready,
    Checking,
    Current,
    Available,
    Downloading,
    Verified,
    Error,
}

pub(crate) struct UpdateController {
    pub(crate) available: Option<VerifiedUpdate>,
    pub(crate) verified_path: Option<PathBuf>,
    pub(crate) busy: bool,
    pub(crate) state: UpdateUiState,
}

impl Default for UpdateController {
    fn default() -> Self {
        Self {
            available: None,
            verified_path: None,
            busy: false,
            state: UpdateUiState::Ready,
        }
    }
}
