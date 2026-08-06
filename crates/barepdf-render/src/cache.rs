use barepdf_core::{DocumentId, MemoryBudget, PageIndex, Rotation};
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
        let bitmap_bytes = bitmap.pixels.len();
        self.evict_for(bitmap_bytes);

        let arc_bitmap = Arc::new(bitmap);
        if let Some(old) = self.cache.put(key, arc_bitmap.clone()) {
            self.current_bytes = self.current_bytes.saturating_sub(old.pixels.len());
        }
        self.current_bytes += bitmap_bytes;
        arc_bitmap
    }

    pub fn evict_for(&mut self, required_bytes: usize) {
        while self.current_bytes + required_bytes > self.budget_bytes && !self.cache.is_empty() {
            if let Some((_, popped)) = self.cache.pop_lru() {
                self.current_bytes = self.current_bytes.saturating_sub(popped.pixels.len());
            }
        }
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.current_bytes = 0;
    }

    pub fn current_bytes(&self) -> usize {
        self.current_bytes
    }
}

#[derive(Clone)]
pub struct SharedBitmapCache {
    inner: Arc<Mutex<BitmapCache>>,
}

impl SharedBitmapCache {
    pub fn new(budget: MemoryBudget) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BitmapCache::new(budget))),
        }
    }

    pub fn get(&self, key: &CacheKey) -> Option<Arc<RawBitmap>> {
        if let Ok(mut cache) = self.inner.lock() {
            cache.get(key)
        } else {
            None
        }
    }

    pub fn insert(&self, key: CacheKey, bitmap: RawBitmap) -> Arc<RawBitmap> {
        let mut cache = self.inner.lock().expect("cache lock");
        cache.insert(key, bitmap)
    }

    pub fn clear(&self) {
        if let Ok(mut cache) = self.inner.lock() {
            cache.clear();
        }
    }
}
