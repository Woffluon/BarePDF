use barepdf_core::{DocumentId, MemoryBudget, PageIndex, PdfError, Rotation};
use barepdf_pdf::RawBitmap;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub document_id: DocumentId,
    pub page_index: PageIndex,
    pub target_width: u32,
    pub target_height: u32,
    pub rotation: Rotation,
}

pub struct BitmapCache {
    cache: LruCache<CacheKey, Arc<RawBitmap>>,
    current_bytes: usize,
    budget_bytes: usize,
}

impl BitmapCache {
    #[must_use]
    /// # Panics
    ///
    /// This cannot panic: the fixed cache entry count is non-zero.
    pub fn new(budget: MemoryBudget) -> Self {
        Self {
            // High capacity bound; memory byte budget controls eviction
            cache: LruCache::new(NonZeroUsize::new(1000).expect("non-zero")),
            current_bytes: 0,
            budget_bytes: budget.get(),
        }
    }

    pub fn get(&mut self, key: &CacheKey) -> Option<Arc<RawBitmap>> {
        self.cache.get(key).cloned()
    }

    pub fn insert(&mut self, key: CacheKey, bitmap: RawBitmap) -> Arc<RawBitmap> {
        let bitmap_bytes = bitmap.pixels().len();
        let arc_bitmap = Arc::new(bitmap);
        if bitmap_bytes > self.budget_bytes {
            return arc_bitmap;
        }
        self.evict_for(bitmap_bytes);

        if let Some((_, old)) = self.cache.push(key, arc_bitmap.clone()) {
            self.current_bytes = self.current_bytes.saturating_sub(old.pixels().len());
        }
        self.current_bytes += bitmap_bytes;
        arc_bitmap
    }

    pub fn evict_for(&mut self, required_bytes: usize) {
        while self.current_bytes.saturating_add(required_bytes) > self.budget_bytes
            && !self.cache.is_empty()
        {
            if let Some((_, popped)) = self.cache.pop_lru() {
                self.current_bytes = self.current_bytes.saturating_sub(popped.pixels().len());
            }
        }
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.current_bytes = 0;
    }

    #[must_use]
    pub const fn current_bytes(&self) -> usize {
        self.current_bytes
    }
}

#[derive(Clone)]
pub struct SharedBitmapCache {
    inner: Arc<Mutex<BitmapCache>>,
}

impl SharedBitmapCache {
    #[must_use]
    pub fn new(budget: MemoryBudget) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BitmapCache::new(budget))),
        }
    }

    /// # Errors
    ///
    /// Returns `PdfError::CacheError` if the shared cache lock is poisoned.
    pub fn get(&self, key: &CacheKey) -> Result<Option<Arc<RawBitmap>>, PdfError> {
        let mut cache = self
            .inner
            .lock()
            .map_err(|_| PdfError::CacheError("bitmap cache lock poisoned".into()))?;
        Ok(cache.get(key))
    }

    /// # Errors
    ///
    /// Returns `PdfError::CacheError` if the shared cache lock is poisoned.
    pub fn insert(&self, key: CacheKey, bitmap: RawBitmap) -> Result<Arc<RawBitmap>, PdfError> {
        let mut cache = self
            .inner
            .lock()
            .map_err(|_| PdfError::CacheError("bitmap cache lock poisoned".into()))?;
        Ok(cache.insert(key, bitmap))
    }

    /// # Errors
    ///
    /// Returns `PdfError::CacheError` if the shared cache lock is poisoned.
    pub fn clear(&self) -> Result<(), PdfError> {
        let mut cache = self
            .inner
            .lock()
            .map_err(|_| PdfError::CacheError("bitmap cache lock poisoned".into()))?;
        cache.clear();
        Ok(())
    }
}
