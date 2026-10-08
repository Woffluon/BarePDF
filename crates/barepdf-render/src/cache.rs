use barepdf_core::{DocumentId, MemoryBudget, PageIndex, Rotation};
use barepdf_pdf::RawBitmap;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub document_id: DocumentId,
    pub page_index: PageIndex,
    pub target_width: u32,
    pub target_height: u32,
    pub rotation: Rotation,
    pub invert_colors: bool,
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
        if let Some(old) = self.cache.pop(&key) {
            self.current_bytes = self.current_bytes.saturating_sub(old.pixels().len());
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

    pub fn evict_to_budget(&mut self) {
        while self.current_bytes > self.budget_bytes && !self.cache.is_empty() {
            if let Some((_, popped)) = self.cache.pop_lru() {
                self.current_bytes = self.current_bytes.saturating_sub(popped.pixels().len());
            }
        }
    }

    pub fn set_budget(&mut self, budget: MemoryBudget) {
        self.budget_bytes = budget.get();
        self.evict_to_budget();
    }

    #[must_use]
    pub const fn budget(&self) -> MemoryBudget {
        MemoryBudget::new(self.budget_bytes)
    }

    pub fn evict_document(&mut self, document_id: DocumentId) {
        while let Some(key) = self
            .cache
            .iter()
            .find_map(|(k, _)| (k.document_id == document_id).then_some(*k))
        {
            if let Some(popped) = self.cache.pop(&key) {
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
            invert_colors: false,
        }
    }

    fn bitmap() -> RawBitmap {
        RawBitmap::new(1, 1, vec![0; 4]).expect("one RGBA pixel is a valid bitmap")
    }

    fn bitmap_2x1() -> RawBitmap {
        RawBitmap::new(2, 1, vec![0; 8]).expect("two RGBA pixels are a valid bitmap")
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

    #[test]
    fn evict_document_removes_only_target_document_and_drops_bitmaps() {
        let mut cache = BitmapCache::new(MemoryBudget::new(100));
        let weak_p1 = Arc::downgrade(&cache.insert(key(1), bitmap()));
        let weak_p2 = Arc::downgrade(&cache.insert(key(2), bitmap()));
        let mut key_doc2 = key(1);
        key_doc2.document_id = DocumentId::new(2);
        let weak_doc2 = Arc::downgrade(&cache.insert(key_doc2, bitmap()));

        assert_eq!(cache.current_bytes(), 12);
        cache.evict_document(DocumentId::new(1));
        assert_eq!(cache.current_bytes(), 4);
        assert!(cache.get(&key(1)).is_none());
        assert!(cache.get(&key(2)).is_none());
        assert!(cache.get(&key_doc2).is_some());
        assert!(weak_p1.upgrade().is_none());
        assert!(weak_p2.upgrade().is_none());
        assert!(weak_doc2.upgrade().is_some());
    }

    #[test]
    fn updating_existing_key_does_not_evict_other_entries_if_budget_allows() {
        let mut cache = BitmapCache::new(MemoryBudget::new(12));
        cache.insert(key(1), bitmap());
        cache.insert(key(2), bitmap());
        assert!(cache.get(&key(1)).is_some());
        assert!(cache.get(&key(2)).is_some());
        assert_eq!(cache.current_bytes(), 8);

        // Update key(2) with an 8-byte bitmap (4 + 8 == 12 <= budget).
        // If old key(2) were not popped before evict_for(8), 8 + 8 = 16 > 12 would evict key(1).
        cache.insert(key(2), bitmap_2x1());

        assert!(
            cache.get(&key(1)).is_some(),
            "key(1) was prematurely evicted when updating key(2)"
        );
        assert_eq!(cache.get(&key(2)).map(|b| b.pixels().len()), Some(8));
        assert_eq!(cache.current_bytes(), 12);
    }

    #[test]
    fn evicted_and_cleared_bitmaps_are_dropped_immediately() {
        let mut cache = BitmapCache::new(MemoryBudget::new(8));

        let weak_1 = Arc::downgrade(&cache.insert(key(1), bitmap()));
        let weak_2 = Arc::downgrade(&cache.insert(key(2), bitmap()));
        assert_eq!(cache.current_bytes(), 8);
        assert!(weak_1.upgrade().is_some());

        // Inserting key(3) evicts key(1) and drops its allocation immediately
        let weak_3 = Arc::downgrade(&cache.insert(key(3), bitmap()));
        assert_eq!(cache.current_bytes(), 8);
        assert!(weak_1.upgrade().is_none());
        assert!(weak_2.upgrade().is_some());
        assert!(weak_3.upgrade().is_some());

        cache.clear();
        assert_eq!(cache.current_bytes(), 0);
        assert!(weak_2.upgrade().is_none());
        assert!(weak_3.upgrade().is_none());
    }

    #[test]
    fn test_cache_set_budget_evicts_down_to_new_budget() {
        let mut cache = BitmapCache::new(MemoryBudget::new(12));
        cache.insert(key(1), bitmap());
        cache.insert(key(2), bitmap());
        cache.insert(key(3), bitmap());
        assert_eq!(cache.current_bytes(), 12);

        cache.set_budget(MemoryBudget::new(8));
        assert_eq!(cache.budget().get(), 8);
        assert_eq!(cache.current_bytes(), 8);
        assert!(cache.get(&key(1)).is_none());
        assert!(cache.get(&key(2)).is_some());
        assert!(cache.get(&key(3)).is_some());
    }

    #[test]
    fn invert_colors_keys_are_cached_independently() {
        let mut cache = BitmapCache::new(MemoryBudget::new(16));
        let normal_key = key(1);
        let mut inverted_key = key(1);
        inverted_key.invert_colors = true;

        let normal_bmp =
            RawBitmap::new(1, 1, vec![10, 20, 30, 255]).expect("valid normal RGBA bitmap");
        let inverted_bmp =
            RawBitmap::new(1, 1, vec![245, 235, 225, 255]).expect("valid inverted RGBA bitmap");

        cache.insert(normal_key, normal_bmp);
        cache.insert(inverted_key, inverted_bmp);

        assert_eq!(cache.current_bytes(), 8);
        assert_eq!(
            cache.get(&normal_key).map(|b| b.pixels().to_vec()),
            Some(vec![10, 20, 30, 255])
        );
        assert_eq!(
            cache.get(&inverted_key).map(|b| b.pixels().to_vec()),
            Some(vec![245, 235, 225, 255])
        );
    }
}
