use crate::error::RenderError;
use barepdf_core::{DocumentId, PageIndex, RequestId, Rotation};
use barepdf_pdf::{OutlineNode, RawBitmap, TextSpan};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Visible = 0,
    Prefetch = 1,
    Thumbnail = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderKind {
    Page,
    Thumbnail,
}

#[derive(Debug, Clone)]
pub struct RenderJob {
    pub request_id: RequestId,
    pub generation: u64,
    pub document_id: DocumentId,
    pub page_index: PageIndex,
    pub target_width: u32,
    pub target_height: u32,
    pub rotation: Rotation,
    pub priority: Priority,
    pub kind: RenderKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RenderRequestKey {
    pub(crate) generation: u64,
    pub(crate) document_id: DocumentId,
    pub(crate) page_index: PageIndex,
    pub(crate) target_width: u32,
    pub(crate) target_height: u32,
    pub(crate) rotation: Rotation,
    pub(crate) kind: RenderKind,
}

impl From<&RenderJob> for RenderRequestKey {
    fn from(job: &RenderJob) -> Self {
        Self {
            generation: job.generation,
            document_id: job.document_id,
            page_index: job.page_index,
            target_width: job.target_width,
            target_height: job.target_height,
            rotation: job.rotation,
            kind: job.kind,
        }
    }
}

pub enum RenderCommand {
    OpenDocument {
        document_id: DocumentId,
        path: PathBuf,
        password: Option<String>,
    },
    RenderPage(RenderJob),
    ExtractText {
        document_id: DocumentId,
        generation: u64,
        page_index: PageIndex,
    },
    FetchTextGeometry {
        document_id: DocumentId,
        generation: u64,
        page_index: PageIndex,
    },
    FetchOutline {
        document_id: DocumentId,
    },
    FetchPageDimensions {
        document_id: DocumentId,
        start: u32,
        count: u32,
    },
    CloseDocument(DocumentId),
    Shutdown,
}

pub enum RenderEvent {
    DocumentOpened {
        document_id: DocumentId,
        page_count: u32,
        first_page_dimensions: (f32, f32),
    },
    PageRendered {
        request_id: RequestId,
        generation: u64,
        document_id: DocumentId,
        page_index: PageIndex,
        kind: RenderKind,
        bitmap: Arc<RawBitmap>,
    },
    TextExtracted {
        document_id: DocumentId,
        generation: u64,
        page_index: PageIndex,
        text: String,
        spans: Vec<TextSpan>,
    },
    TextGeometryFetched {
        document_id: DocumentId,
        generation: u64,
        page_index: PageIndex,
        geometry: barepdf_core::PageTextGeometry,
    },
    OutlineFetched {
        document_id: DocumentId,
        outline: Vec<OutlineNode>,
    },
    PageDimensionsFetched {
        document_id: DocumentId,
        start: u32,
        dimensions: Vec<(f32, f32)>,
    },
    Error {
        request_id: Option<RequestId>,
        document_id: DocumentId,
        generation: Option<u64>,
        error: RenderError,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(generation: u64, kind: RenderKind) -> RenderJob {
        RenderJob {
            request_id: RequestId::new(1),
            generation,
            document_id: DocumentId::new(7),
            page_index: PageIndex::from_raw(2),
            target_width: 800,
            target_height: 1000,
            rotation: Rotation::Degrees0,
            priority: Priority::Visible,
            kind,
        }
    }

    #[test]
    fn render_request_key_includes_generation_and_kind() {
        assert_ne!(
            RenderRequestKey::from(&job(1, RenderKind::Page)),
            RenderRequestKey::from(&job(2, RenderKind::Page))
        );
        assert_ne!(
            RenderRequestKey::from(&job(1, RenderKind::Page)),
            RenderRequestKey::from(&job(1, RenderKind::Thumbnail))
        );
    }
}
