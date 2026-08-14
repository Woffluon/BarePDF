use barepdf_core::{PageCount, PageIndex};
use std::num::{NonZeroU16, NonZeroU64};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Copies(NonZeroU16);

impl Copies {
    pub const MAX: u16 = 99;

    /// # Errors
    ///
    /// Returns [`PrintError::InvalidCopies`] unless `copies` is in `1..=99`.
    pub fn new(copies: u16) -> Result<Self, PrintError> {
        NonZeroU16::new(copies)
            .filter(|copies| copies.get() <= Self::MAX)
            .map(Self)
            .ok_or(PrintError::InvalidCopies(copies))
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

impl Default for Copies {
    fn default() -> Self {
        Self(NonZeroU16::MIN)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrintRange {
    first: PageIndex,
    last: PageIndex,
}

impl PrintRange {
    /// Creates an inclusive range whose indices belong to `page_count`.
    ///
    /// # Errors
    ///
    /// Returns [`PrintError::InvalidRange`] for reversed or out-of-bounds ranges.
    pub fn new(
        first: PageIndex,
        last: PageIndex,
        page_count: PageCount,
    ) -> Result<Self, PrintError> {
        (first <= last && last.get() < page_count.get())
            .then_some(Self { first, last })
            .ok_or(PrintError::InvalidRange)
    }

    #[must_use]
    pub fn all(page_count: PageCount) -> Self {
        Self {
            first: PageIndex::zero(),
            last: PageIndex::from_raw(page_count.get() - 1),
        }
    }

    #[must_use]
    pub const fn first(self) -> PageIndex {
        self.first
    }

    #[must_use]
    pub const fn last(self) -> PageIndex {
        self.last
    }

    pub fn pages(self) -> impl Iterator<Item = PageIndex> {
        (self.first.get()..=self.last.get()).map(PageIndex::from_raw)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrintJobId(NonZeroU64);

impl PrintJobId {
    #[must_use]
    pub const fn new(id: u64) -> Option<Self> {
        match NonZeroU64::new(id) {
            Some(id) => Some(Self(id)),
            None => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrintPage<'a> {
    width: u32,
    height: u32,
    bgra: &'a [u8],
}

impl<'a> PrintPage<'a> {
    /// Validates a tightly packed, 32-bit BGRA raster page.
    ///
    /// # Errors
    ///
    /// Returns [`PrintError::InvalidPage`] for zero dimensions, arithmetic overflow, or a byte
    /// length other than `width * height * 4`.
    pub fn new(width: u32, height: u32, bgra: &'a [u8]) -> Result<Self, PrintError> {
        let expected = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4));
        if width == 0 || height == 0 || expected != Some(bgra.len()) {
            return Err(PrintError::InvalidPage);
        }
        Ok(Self {
            width,
            height,
            bgra,
        })
    }

    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }

    #[must_use]
    pub const fn bgra(self) -> &'a [u8] {
        self.bgra
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrintError {
    #[error("invalid print copy count: {0}")]
    InvalidCopies(u16),
    #[error("invalid print page range")]
    InvalidRange,
    #[error("invalid print page bitmap")]
    InvalidPage,
    #[error("invalid print DPI: {0}")]
    InvalidDpi(u16),
    #[error("print dialog failed with code {0}")]
    Dialog(u32),
    #[error("Windows print operation {operation} failed with code {code}")]
    Platform { operation: &'static str, code: u32 },
    #[error("invalid printer sink state")]
    InvalidState,
}

pub trait PrinterSink: Send {
    fn job_id(&self) -> PrintJobId;
    fn target_dpi(&self) -> u16;

    /// # Errors
    ///
    /// Returns an error when the platform cannot start the spool document.
    fn begin(&mut self, title: &str) -> Result<(), PrintError>;

    /// # Errors
    ///
    /// Returns an error when the platform cannot spool the page.
    fn write_page(&mut self, page: PrintPage<'_>) -> Result<(), PrintError>;

    /// # Errors
    ///
    /// Returns an error when the platform cannot finish the spool document.
    fn finish(self: Box<Self>) -> Result<(), PrintError>;
}

pub struct PrintSelection<S> {
    pub sink: S,
    pub range: PrintRange,
    pub copies: Copies,
}

pub trait PrinterDialog {
    type Sink: PrinterSink;

    /// Shows the platform printer chooser. User cancellation returns `Ok(None)`.
    ///
    /// # Errors
    ///
    /// Returns an error when the native dialog fails or returns invalid settings.
    fn select(
        &mut self,
        job_id: PrintJobId,
        page_count: PageCount,
    ) -> Result<Option<PrintSelection<Self::Sink>>, PrintError>;
}

#[cfg(test)]
mod tests {
    use super::{Copies, PrintError, PrintJobId, PrintPage, PrintRange, PrinterSink};
    use barepdf_core::{PageCount, PageIndex};

    #[test]
    fn copies_accepts_only_one_through_ninety_nine() {
        assert_eq!(Copies::new(1).map(Copies::get), Ok(1));
        assert_eq!(Copies::new(99).map(Copies::get), Ok(99));
        assert_eq!(Copies::new(0), Err(PrintError::InvalidCopies(0)));
        assert_eq!(Copies::new(100), Err(PrintError::InvalidCopies(100)));
    }

    #[test]
    fn range_rejects_reverse_and_out_of_document_indices() {
        let page_count = PageCount::new(3).unwrap();
        assert!(
            PrintRange::new(PageIndex::from_raw(1), PageIndex::from_raw(0), page_count).is_err()
        );
        assert!(PrintRange::new(PageIndex::zero(), PageIndex::from_raw(3), page_count).is_err());
        assert_eq!(
            PrintRange::all(page_count)
                .pages()
                .map(PageIndex::get)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn page_requires_exact_bgra_storage() {
        let bytes = [0_u8; 16];
        assert!(PrintPage::new(2, 2, &bytes).is_ok());
        assert_eq!(
            PrintPage::new(2, 2, &bytes[..15]),
            Err(PrintError::InvalidPage)
        );
        assert_eq!(PrintPage::new(0, 2, &[]), Err(PrintError::InvalidPage));
    }

    struct FakePrinterSink {
        job_id: PrintJobId,
        started: bool,
    }

    impl PrinterSink for FakePrinterSink {
        fn job_id(&self) -> PrintJobId {
            self.job_id
        }

        fn target_dpi(&self) -> u16 {
            300
        }

        fn begin(&mut self, _title: &str) -> Result<(), PrintError> {
            self.started = true;
            Ok(())
        }

        fn write_page(&mut self, _page: PrintPage<'_>) -> Result<(), PrintError> {
            self.started.then_some(()).ok_or(PrintError::InvalidState)
        }

        fn finish(self: Box<Self>) -> Result<(), PrintError> {
            self.started.then_some(()).ok_or(PrintError::InvalidState)
        }
    }

    #[test]
    fn safe_sink_contract_supports_one_page_flow() {
        let Some(job_id) = PrintJobId::new(1) else {
            panic!("one is a valid non-zero print job ID");
        };
        let mut sink: Box<dyn PrinterSink> = Box::new(FakePrinterSink {
            job_id,
            started: false,
        });
        let pixels = [255_u8; 4];
        let page = PrintPage::new(1, 1, &pixels).unwrap();

        assert_eq!(sink.job_id(), job_id);
        assert!(sink.begin("test").is_ok());
        assert!(sink.write_page(page).is_ok());
        assert!(sink.finish().is_ok());
        assert!(PrintJobId::new(0).is_none());
    }
}
