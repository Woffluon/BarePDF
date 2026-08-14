use barepdf_core::{DocumentId, MemoryBudget, PageIndex, Rotation};
use barepdf_pdf::RawBitmap;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;

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
    pub fn new(budget: MemoryBudget) -> Self {
        Self {
            // High capacity bound; memory byte budget controls eviction
            cache: LruCache::new(NonZeroUsize::new(1000).unwrap_or(NonZeroUsize::MIN)),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn key(page: u32) -> CacheKey {
        CacheKey {
            document_id: DocumentId::new(1),
            page_index: PageIndex::from_raw(page),
            target_width: 1,
            target_height: 1,
            rotation: Rotation::Degrees0,
        }
    }

    fn bitmap() -> RawBitmap {
        RawBitmap::new(1, 1, vec![0; 4]).expect("one RGBA pixel is a valid bitmap")
    }

    #[test]
    fn byte_budget_evicts_least_recently_used_bitmap() {
        let mut cache = BitmapCache::new(MemoryBudget::new(8));
        cache.insert(key(1), bitmap());
        cache.insert(key(2), bitmap());
        assert!(cache.get(&key(1)).is_some());
        cache.insert(key(3), bitmap());

        assert_eq!(cache.current_bytes(), 8);
        assert!(cache.get(&key(1)).is_some());
        assert!(cache.get(&key(2)).is_none());
        assert!(cache.get(&key(3)).is_some());
    }

    #[test]
    fn oversized_bitmap_is_returned_without_being_cached() {
        let mut cache = BitmapCache::new(MemoryBudget::new(3));
        let bitmap = cache.insert(key(1), bitmap());

        assert_eq!(bitmap.pixels().len(), 4);
        assert_eq!(cache.current_bytes(), 0);
        assert!(cache.get(&key(1)).is_none());
    }
}
