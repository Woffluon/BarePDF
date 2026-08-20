use std::sync::atomic::{AtomicU64, Ordering};

const TRACE_TARGET: &str = "barepdf_render::observability";
const SAMPLE_EVERY: u64 = 64;

#[derive(Default)]
pub(crate) struct RenderObservability {
    queue_full: AtomicU64,
    queue_disconnected: AtomicU64,
    dropped_events: AtomicU64,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    stale_work: AtomicU64,
}

impl RenderObservability {
    pub(crate) fn queue_full(&self, queue: &'static str) {
        if !tracing::enabled!(target: TRACE_TARGET, tracing::Level::WARN) {
            return;
        }
        if let Some(count) = sample(&self.queue_full) {
            tracing::warn!(target: TRACE_TARGET, queue, count, "render command queue is full");
        }
    }

    pub(crate) fn queue_disconnected(&self, queue: &'static str) {
        if !tracing::enabled!(target: TRACE_TARGET, tracing::Level::WARN) {
            return;
        }
        if let Some(count) = sample(&self.queue_disconnected) {
            tracing::warn!(target: TRACE_TARGET, queue, count, "render command queue is disconnected");
        }
    }

    pub(crate) fn event_dropped(&self) {
        if !tracing::enabled!(target: TRACE_TARGET, tracing::Level::WARN) {
            return;
        }
        if let Some(count) = sample(&self.dropped_events) {
            tracing::warn!(target: TRACE_TARGET, count, "render event dropped because the event queue is full");
        }
    }

    pub(crate) fn cache_hit(&self) {
        if !tracing::enabled!(target: TRACE_TARGET, tracing::Level::DEBUG) {
            return;
        }
        if let Some(count) = sample(&self.cache_hits) {
            tracing::debug!(target: TRACE_TARGET, count, "render bitmap cache hit");
        }
    }

    pub(crate) fn cache_miss(&self) {
        if !tracing::enabled!(target: TRACE_TARGET, tracing::Level::DEBUG) {
            return;
        }
        if let Some(count) = sample(&self.cache_misses) {
            tracing::debug!(target: TRACE_TARGET, count, "render bitmap cache miss");
        }
    }

    pub(crate) fn stale_work(&self, reason: &'static str) {
        if !tracing::enabled!(target: TRACE_TARGET, tracing::Level::DEBUG) {
            return;
        }
        if let Some(count) = sample(&self.stale_work) {
            tracing::debug!(target: TRACE_TARGET, reason, count, "stale render work ignored");
        }
    }
}

fn sample(counter: &AtomicU64) -> Option<u64> {
    let count = counter.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    (count == 1 || count.is_multiple_of(SAMPLE_EVERY)).then_some(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampling_keeps_the_first_signal_and_then_every_sixty_fourth() {
        let counter = AtomicU64::new(0);

        assert_eq!(sample(&counter), Some(1));
        for _ in 0..62 {
            assert_eq!(sample(&counter), None);
        }
        assert_eq!(sample(&counter), Some(64));
    }
}
