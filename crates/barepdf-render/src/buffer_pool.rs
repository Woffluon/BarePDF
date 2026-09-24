use std::sync::Mutex;

const MAX_POOLED_BUFFERS_PER_BUCKET: usize = 4;

const RESOLUTION_1080P_BYTES: usize = 1920 * 1080 * 4;
const RESOLUTION_1440P_BYTES: usize = 2560 * 1440 * 4;
const RESOLUTION_4K_BYTES: usize = 3840 * 2160 * 4;

pub struct BitmapBufferPool {
    pool_1080p: Mutex<Vec<Vec<u8>>>,
    pool_1440p: Mutex<Vec<Vec<u8>>>,
    pool_4k: Mutex<Vec<Vec<u8>>>,
}

impl Default for BitmapBufferPool {
    fn default() -> Self {
        Self::new()
    }
}

impl BitmapBufferPool {
    #[must_use]
    pub fn new() -> Self {
        Self {
            pool_1080p: Mutex::new(Vec::with_capacity(MAX_POOLED_BUFFERS_PER_BUCKET)),
            pool_1440p: Mutex::new(Vec::with_capacity(MAX_POOLED_BUFFERS_PER_BUCKET)),
            pool_4k: Mutex::new(Vec::with_capacity(MAX_POOLED_BUFFERS_PER_BUCKET)),
        }
    }

    #[must_use]
    pub fn checkout(&self, required_bytes: usize) -> Vec<u8> {
        let pool = if required_bytes <= RESOLUTION_1080P_BYTES {
            Some(&self.pool_1080p)
        } else if required_bytes <= RESOLUTION_1440P_BYTES {
            Some(&self.pool_1440p)
        } else if required_bytes <= RESOLUTION_4K_BYTES {
            Some(&self.pool_4k)
        } else {
            None
        };

        if let Some(pool) = pool {
            if let Ok(mut lock) = pool.lock() {
                if let Some(mut buf) = lock.pop() {
                    buf.resize(required_bytes, 0);
                    return buf;
                }
            }
        }
        vec![0; required_bytes]
    }

    pub fn recycle(&self, mut buffer: Vec<u8>) {
        let capacity = buffer.capacity();
        let pool = if capacity <= RESOLUTION_1080P_BYTES {
            Some(&self.pool_1080p)
        } else if capacity <= RESOLUTION_1440P_BYTES {
            Some(&self.pool_1440p)
        } else if capacity <= RESOLUTION_4K_BYTES {
            Some(&self.pool_4k)
        } else {
            None
        };

        if let Some(pool) = pool {
            if let Ok(mut lock) = pool.lock() {
                if lock.len() < MAX_POOLED_BUFFERS_PER_BUCKET {
                    buffer.clear();
                    lock.push(buffer);
                }
            }
        }
    }

    pub fn copy_bgra_to_rgba(&self, bgra: &[u8], target: &mut [u8]) {
        let len = bgra.len().min(target.len());
        let mut i = 0;
        while i + 4 <= len {
            target[i] = bgra[i + 2];
            target[i + 1] = bgra[i + 1];
            target[i + 2] = bgra[i];
            target[i + 3] = bgra[i + 3];
            i += 4;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool_checkout_recycle() {
        let pool = BitmapBufferPool::new();
        let req = 1024;
        let buf = pool.checkout(req);
        assert_eq!(buf.len(), req);
        pool.recycle(buf);
        let buf2 = pool.checkout(req);
        assert_eq!(buf2.len(), req);
    }
}
