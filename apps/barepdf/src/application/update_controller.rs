use crate::infrastructure::VerifiedUpdate;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateConsent {
    Undecided,
    Disabled,
    Enabled,
}

impl From<Option<bool>> for UpdateConsent {
    fn from(persisted: Option<bool>) -> Self {
        match persisted {
            None => Self::Undecided,
            Some(false) => Self::Disabled,
            Some(true) => Self::Enabled,
        }
    }
}

impl UpdateConsent {
    pub(crate) const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateUiState {
    Ready,
    Checking,
    Current,
    Available,
    Downloading,
    Installing,
    Verified,
    Error,
}

enum UpdateLifecycle {
    Ready,
    Checking,
    Current,
    Available(VerifiedUpdate),
    Downloading(VerifiedUpdate),
    Installing(VerifiedUpdate),
    Verified {
        path: PathBuf,
        update: VerifiedUpdate,
    },
    Error,
}

pub(crate) struct UpdateController {
    lifecycle: UpdateLifecycle,
}

impl Default for UpdateController {
    fn default() -> Self {
        Self {
            lifecycle: UpdateLifecycle::Ready,
        }
    }
}

impl UpdateController {
    pub(crate) const fn is_busy(&self) -> bool {
        matches!(
            &self.lifecycle,
            UpdateLifecycle::Checking
                | UpdateLifecycle::Downloading(_)
                | UpdateLifecycle::Installing(_)
        )
    }

    pub(crate) const fn ui_state(&self) -> UpdateUiState {
        match &self.lifecycle {
            UpdateLifecycle::Ready => UpdateUiState::Ready,
            UpdateLifecycle::Checking => UpdateUiState::Checking,
            UpdateLifecycle::Current => UpdateUiState::Current,
            UpdateLifecycle::Available(_) => UpdateUiState::Available,
            UpdateLifecycle::Downloading(_) => UpdateUiState::Downloading,
            UpdateLifecycle::Installing(_) => UpdateUiState::Installing,
            UpdateLifecycle::Verified { .. } => UpdateUiState::Verified,
            UpdateLifecycle::Error => UpdateUiState::Error,
        }
    }

    pub(crate) fn begin_check(&mut self) -> bool {
        if self.is_busy() {
            return false;
        }
        self.lifecycle = UpdateLifecycle::Checking;
        true
    }

    pub(crate) fn mark_current(&mut self) {
        self.lifecycle = UpdateLifecycle::Current;
    }

    pub(crate) fn mark_available(&mut self, update: VerifiedUpdate) {
        self.lifecycle = UpdateLifecycle::Available(update);
    }

    pub(crate) fn mark_downloaded(&mut self, path: PathBuf, update: VerifiedUpdate) {
        self.lifecycle = UpdateLifecycle::Verified { path, update };
    }

    pub(crate) fn mark_failed(&mut self) {
        self.lifecycle = UpdateLifecycle::Error;
    }

    pub(crate) fn available_update(&self) -> Option<&VerifiedUpdate> {
        let UpdateLifecycle::Available(update) = &self.lifecycle else {
            return None;
        };
        Some(update)
    }

    pub(crate) fn allows_automatic_checks(&self, persisted_consent: Option<bool>) -> bool {
        !self.is_busy() && UpdateConsent::from(persisted_consent).is_enabled()
    }

    pub(crate) fn release_url(&self) -> Option<&str> {
        match &self.lifecycle {
            UpdateLifecycle::Available(update)
            | UpdateLifecycle::Downloading(update)
            | UpdateLifecycle::Installing(update)
            | UpdateLifecycle::Verified { update, .. } => Some(update.release_url()),
            UpdateLifecycle::Ready
            | UpdateLifecycle::Checking
            | UpdateLifecycle::Current
            | UpdateLifecycle::Error => None,
        }
    }

    pub(crate) fn begin_download(&mut self) -> Option<VerifiedUpdate> {
        let UpdateLifecycle::Available(update) = &self.lifecycle else {
            return None;
        };
        let update = update.clone();
        self.lifecycle = UpdateLifecycle::Downloading(update.clone());
        Some(update)
    }

    pub(crate) fn begin_install(&mut self) -> Option<(PathBuf, VerifiedUpdate)> {
        let UpdateLifecycle::Verified { path, update } = &self.lifecycle else {
            return None;
        };
        let path = path.clone();
        let update = update.clone();
        self.lifecycle = UpdateLifecycle::Installing(update.clone());
        Some((path, update))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_maps_the_persisted_three_state_value_without_a_second_field() {
        assert_eq!(UpdateConsent::from(None), UpdateConsent::Undecided);
        assert_eq!(UpdateConsent::from(Some(false)), UpdateConsent::Disabled);
        assert_eq!(UpdateConsent::from(Some(true)), UpdateConsent::Enabled);
        assert!(!UpdateConsent::Undecided.is_enabled());
        assert!(!UpdateConsent::Disabled.is_enabled());
        assert!(UpdateConsent::Enabled.is_enabled());
    }

    #[test]
    fn lifecycle_keeps_busy_state_in_its_variant() {
        let mut updates = UpdateController::default();

        assert_eq!(updates.ui_state(), UpdateUiState::Ready);
        assert!(!updates.is_busy());
        assert!(updates.begin_check());
        assert_eq!(updates.ui_state(), UpdateUiState::Checking);
        assert!(updates.is_busy());
        assert!(!updates.begin_check());

        updates.mark_current();
        assert_eq!(updates.ui_state(), UpdateUiState::Current);
        assert!(!updates.is_busy());
    }

    #[test]
    fn failed_operations_leave_no_busy_flag_behind() {
        let mut updates = UpdateController::default();
        assert!(updates.begin_check());

        updates.mark_failed();

        assert_eq!(updates.ui_state(), UpdateUiState::Error);
        assert!(!updates.is_busy());
    }
}
