use super::DocumentState;
use barepdf_core::{PageIndex, Rotation, ZoomFactor, ZoomMode, MAX_OPEN_TABS};
use barepdf_pdf::OutlineNode;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TabId(NonZeroU64);

impl TabId {
    #[must_use]
    pub(crate) const fn get(self) -> u64 {
        self.0.get()
    }

    #[must_use]
    pub(crate) fn to_slint_id(self) -> i32 {
        i32::try_from(self.0.get()).unwrap_or(i32::MAX)
    }

    #[must_use]
    pub(crate) fn from_slint_id(id: i32) -> Option<Self> {
        let raw = u64::try_from(id).ok()?;
        let non_zero = NonZeroU64::new(raw)?;
        Some(Self(non_zero))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ViewState {
    pub(crate) current_page: PageIndex,
    pub(crate) zoom_mode: ZoomMode,
    pub(crate) zoom_factor: ZoomFactor,
    pub(crate) rotation: Rotation,
    pub(crate) scroll_y: f32,
    pub(crate) sidebar_visible: bool,
    pub(crate) sidebar_tab: i32,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            current_page: PageIndex::zero(),
            zoom_mode: ZoomMode::FitPage,
            zoom_factor: ZoomFactor::default(),
            rotation: Rotation::default(),
            scroll_y: 0.0,
            sidebar_visible: true,
            sidebar_tab: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TabDocumentLayout {
    pub(crate) page_dimensions: Arc<Vec<(f32, f32)>>,
    pub(crate) first_page_dimensions: (f32, f32),
    pub(crate) dimensions_revision: u64,
    pub(crate) next_dimensions_start: u32,
    pub(crate) outline: Arc<Vec<OutlineNode>>,
}

impl Default for TabDocumentLayout {
    fn default() -> Self {
        Self {
            page_dimensions: Arc::new(Vec::new()),
            first_page_dimensions: (612.0, 792.0),
            dimensions_revision: 0,
            next_dimensions_start: 1,
            outline: Arc::new(Vec::new()),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TabState {
    pub(crate) id: TabId,
    pub(crate) path: Option<PathBuf>,
    pub(crate) title: String,
    pub(crate) document: Option<DocumentState>,
    pub(crate) view: ViewState,
    pub(crate) layout: TabDocumentLayout,
}

impl TabState {
    #[must_use]
    pub(crate) fn is_loading(&self) -> bool {
        matches!(
            self.document,
            Some(DocumentState::Opening { .. } | DocumentState::PasswordRequired { .. })
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenTab {
    Existing(TabId),
    Created(TabId),
    Full,
}

#[derive(Debug, Default)]
pub(crate) struct TabSet {
    tabs: Vec<TabState>,
    active: Option<TabId>,
    next_id: u64,
}

impl TabSet {
    #[must_use]
    pub(crate) fn tabs(&self) -> &[TabState] {
        &self.tabs
    }

    pub(crate) fn tabs_mut(&mut self) -> &mut [TabState] {
        &mut self.tabs
    }

    #[must_use]
    pub(crate) const fn active_id(&self) -> Option<TabId> {
        self.active
    }

    #[must_use]
    pub(crate) fn active(&self) -> Option<&TabState> {
        let active = self.active?;
        self.tabs.iter().find(|tab| tab.id == active)
    }

    pub(crate) fn active_mut(&mut self) -> Option<&mut TabState> {
        let active = self.active?;
        self.tabs.iter_mut().find(|tab| tab.id == active)
    }

    pub(crate) fn open(&mut self, path: PathBuf, title: String) -> OpenTab {
        if let Some(existing) = self
            .tabs
            .iter()
            .find(|tab| tab.path.as_deref() == Some(path.as_path()))
            .map(|tab| tab.id)
        {
            self.active = Some(existing);
            return OpenTab::Existing(existing);
        }
        if let Some(tab) = self.active_mut().filter(|tab| tab.path.is_none()) {
            tab.path = Some(path);
            tab.title = title;
            tab.document = None;
            tab.layout = TabDocumentLayout::default();
            return OpenTab::Existing(tab.id);
        }
        if self.tabs.len() >= MAX_OPEN_TABS {
            return OpenTab::Full;
        }
        let Some(id) = self.next_tab_id() else {
            return OpenTab::Full;
        };
        self.tabs.push(TabState {
            id,
            path: Some(path),
            title,
            document: None,
            view: ViewState::default(),
            layout: TabDocumentLayout::default(),
        });
        self.active = Some(id);
        OpenTab::Created(id)
    }

    pub(crate) fn new_empty(&mut self) -> Option<TabId> {
        if self.tabs.len() >= MAX_OPEN_TABS {
            return None;
        }
        let id = self.next_tab_id()?;
        self.tabs.push(TabState {
            id,
            path: None,
            title: "New tab".to_string(),
            document: None,
            view: ViewState::default(),
            layout: TabDocumentLayout::default(),
        });
        self.active = Some(id);
        Some(id)
    }

    pub(crate) fn activate(&mut self, id: TabId) -> bool {
        if self.tabs.iter().any(|tab| tab.id == id) {
            self.active = Some(id);
            true
        } else {
            false
        }
    }

    pub(crate) fn close(&mut self, id: TabId) -> bool {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return false;
        };
        let was_active = self.active == Some(id);
        self.tabs.remove(index);
        if was_active {
            self.active = self
                .tabs
                .get(index)
                .or_else(|| index.checked_sub(1).and_then(|left| self.tabs.get(left)))
                .map(|tab| tab.id);
        }
        true
    }

    #[must_use]
    pub(crate) fn find_slint_id(&self, raw: i32) -> Option<TabId> {
        let id = TabId::from_slint_id(raw)?;
        self.tabs.iter().find(|tab| tab.id == id).map(|tab| tab.id)
    }

    #[must_use]
    pub(crate) fn path(&self, id: TabId) -> Option<&Path> {
        self.tabs
            .iter()
            .find(|tab| tab.id == id)
            .and_then(|tab| tab.path.as_deref())
    }

    fn next_tab_id(&mut self) -> Option<TabId> {
        self.next_id = self.next_id.checked_add(1)?;
        NonZeroU64::new(self.next_id).map(TabId)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixteenth_tab_is_allowed_and_seventeenth_is_rejected() {
        let mut tabs = TabSet::default();
        for index in 0..MAX_OPEN_TABS {
            assert!(matches!(
                tabs.open(PathBuf::from(format!("{index}.pdf")), index.to_string()),
                OpenTab::Created(_)
            ));
        }
        assert_eq!(
            tabs.open(PathBuf::from("full.pdf"), "full".into()),
            OpenTab::Full
        );
    }

    #[test]
    fn duplicate_path_activates_existing_tab() {
        let mut tabs = TabSet::default();
        let OpenTab::Created(first) = tabs.open(PathBuf::from("same.pdf"), "same".into()) else {
            return;
        };
        assert_eq!(
            tabs.open(PathBuf::from("same.pdf"), "same".into()),
            OpenTab::Existing(first)
        );
        assert_eq!(tabs.tabs().len(), 1);
    }

    #[test]
    fn active_empty_tab_is_reused_for_open() {
        let mut tabs = TabSet::default();
        let empty = tabs.new_empty();

        assert_eq!(
            tabs.open(PathBuf::from("opened.pdf"), "opened".into()),
            empty.map_or(OpenTab::Full, OpenTab::Existing)
        );
        assert_eq!(tabs.tabs().len(), 1);
        assert_eq!(
            tabs.active().and_then(|tab| tab.path.as_deref()),
            Some(Path::new("opened.pdf"))
        );
    }

    #[test]
    fn active_close_prefers_right_then_left_and_last_becomes_empty() {
        let mut tabs = TabSet::default();
        let OpenTab::Created(left) = tabs.open(PathBuf::from("a.pdf"), "a".into()) else {
            return;
        };
        let OpenTab::Created(middle) = tabs.open(PathBuf::from("b.pdf"), "b".into()) else {
            return;
        };
        let OpenTab::Created(right) = tabs.open(PathBuf::from("c.pdf"), "c".into()) else {
            return;
        };
        assert!(tabs.activate(middle));
        assert!(tabs.close(middle));
        assert_eq!(tabs.active_id(), Some(right));
        assert!(tabs.close(right));
        assert_eq!(tabs.active_id(), Some(left));
        assert!(tabs.close(left));
        assert_eq!(tabs.active_id(), None);
    }

    #[test]
    fn repeated_switching_keeps_one_fixed_tab_set() {
        let mut tabs = TabSet::default();
        let OpenTab::Created(first) = tabs.open(PathBuf::from("a.pdf"), "a".into()) else {
            return;
        };
        let OpenTab::Created(second) = tabs.open(PathBuf::from("b.pdf"), "b".into()) else {
            return;
        };
        for _ in 0..100 {
            assert!(tabs.activate(first));
            assert!(tabs.activate(second));
        }
        assert_eq!(tabs.tabs().len(), 2);
    }

    #[test]
    fn switching_restores_each_tabs_view_state() {
        let mut tabs = TabSet::default();
        let OpenTab::Created(first) = tabs.open(PathBuf::from("a.pdf"), "a".into()) else {
            return;
        };
        if let Some(tab) = tabs.active_mut() {
            tab.view.current_page = PageIndex::from_raw(7);
            tab.view.scroll_y = -320.0;
        }
        let OpenTab::Created(second) = tabs.open(PathBuf::from("b.pdf"), "b".into()) else {
            return;
        };
        if let Some(tab) = tabs.active_mut() {
            tab.view.current_page = PageIndex::from_raw(2);
        }

        assert!(tabs.activate(first));
        assert_eq!(
            tabs.active().map(|tab| tab.view.current_page.get()),
            Some(7)
        );
        assert_eq!(tabs.active().map(|tab| tab.view.scroll_y), Some(-320.0));
        assert!(tabs.activate(second));
        assert_eq!(
            tabs.active().map(|tab| tab.view.current_page.get()),
            Some(2)
        );
    }

    #[test]
    fn new_tabs_start_fit_page_and_keep_custom_zoom_per_tab() {
        let mut tabs = TabSet::default();
        let OpenTab::Created(first) = tabs.open(PathBuf::from("a.pdf"), "a".into()) else {
            return;
        };
        assert_eq!(
            tabs.active().map(|tab| tab.view.zoom_mode),
            Some(ZoomMode::FitPage)
        );
        if let Some(tab) = tabs.active_mut() {
            tab.view.zoom_mode = ZoomMode::Custom(ZoomFactor::new(1.5));
        }
        let OpenTab::Created(second) = tabs.open(PathBuf::from("b.pdf"), "b".into()) else {
            return;
        };
        assert_eq!(
            tabs.active().map(|tab| tab.view.zoom_mode),
            Some(ZoomMode::FitPage)
        );
        assert!(tabs.activate(first));
        assert_eq!(
            tabs.active().map(|tab| tab.view.zoom_mode),
            Some(ZoomMode::Custom(ZoomFactor::new(1.5)))
        );
        assert!(tabs.activate(second));
    }

    #[test]
    fn loading_tab_can_be_closed() {
        let mut tabs = TabSet::default();
        let OpenTab::Created(id) = tabs.open(PathBuf::from("a.pdf"), "a".into()) else {
            return;
        };
        if let Some(tab) = tabs.active_mut() {
            tab.document = Some(DocumentState::Opening {
                id: barepdf_core::DocumentId::new(1),
                path: PathBuf::from("a.pdf"),
                started_at: std::time::Instant::now(),
                active: None,
            });
        }
        assert!(tabs.close(id));
        assert!(tabs.tabs().is_empty());
    }

    #[test]
    fn closing_blank_tab_restores_document_neighbor() {
        let mut tabs = TabSet::default();
        let OpenTab::Created(document) = tabs.open(PathBuf::from("a.pdf"), "a".into()) else {
            return;
        };
        let Some(blank) = tabs.new_empty() else {
            return;
        };

        assert!(tabs.close(blank));
        assert_eq!(tabs.active_id(), Some(document));
    }

    #[test]
    fn switching_away_from_password_tab_exposes_only_target_state() {
        let mut tabs = TabSet::default();
        let OpenTab::Created(protected) = tabs.open(PathBuf::from("protected.pdf"), "p".into())
        else {
            return;
        };
        if let Some(tab) = tabs.active_mut() {
            tab.document = Some(DocumentState::PasswordRequired {
                id: barepdf_core::DocumentId::new(protected.get()),
                path: PathBuf::from("protected.pdf"),
                started_at: std::time::Instant::now(),
                active: None,
            });
        }
        let OpenTab::Created(plain) = tabs.open(PathBuf::from("plain.pdf"), "plain".into()) else {
            return;
        };

        assert_eq!(tabs.active_id(), Some(plain));
        assert!(!tabs.active().is_some_and(TabState::is_loading));
    }

    #[test]
    fn tabs_maintain_independent_document_layouts() {
        let mut tabs = TabSet::default();
        let OpenTab::Created(first) = tabs.open(PathBuf::from("first.pdf"), "first".into()) else {
            panic!("failed to open first tab");
        };
        if let Some(tab) = tabs.active_mut() {
            tab.layout.page_dimensions = Arc::new(vec![(500.0, 700.0); 15]);
            tab.layout.first_page_dimensions = (500.0, 700.0);
            tab.layout.dimensions_revision = 3;
            tab.layout.next_dimensions_start = 16;
            tab.layout.outline = Arc::new(vec![OutlineNode {
                title: "Chapter 1".into(),
                page_index: Some(0),
                children: Vec::new(),
            }]);
        }

        let OpenTab::Created(second) = tabs.open(PathBuf::from("second.pdf"), "second".into())
        else {
            panic!("failed to open second tab");
        };
        if let Some(tab) = tabs.active_mut() {
            tab.layout.page_dimensions = Arc::new(vec![(612.0, 792.0); 3]);
            tab.layout.first_page_dimensions = (612.0, 792.0);
            tab.layout.dimensions_revision = 1;
            tab.layout.next_dimensions_start = 4;
        }

        let active = tabs.active().expect("active tab");
        assert_eq!(active.id, second);
        assert_eq!(active.layout.page_dimensions.len(), 3);
        assert_eq!(active.layout.first_page_dimensions, (612.0, 792.0));
        assert_eq!(active.layout.dimensions_revision, 1);
        assert_eq!(active.layout.next_dimensions_start, 4);
        assert!(active.layout.outline.is_empty());

        assert!(tabs.activate(first));
        let active = tabs.active().expect("active tab");
        assert_eq!(active.id, first);
        assert_eq!(active.layout.page_dimensions.len(), 15);
        assert_eq!(active.layout.first_page_dimensions, (500.0, 700.0));
        assert_eq!(active.layout.dimensions_revision, 3);
        assert_eq!(active.layout.next_dimensions_start, 16);
        assert_eq!(active.layout.outline.len(), 1);
        assert_eq!(active.layout.outline[0].title, "Chapter 1");
    }
}
