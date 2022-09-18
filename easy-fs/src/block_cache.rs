use crate::Block;
use crate::BLOCK_SIZE;
use crate::block_dev::BlockDevice;
use alloc::sync::Arc;
use alloc::collections::VecDeque;
use spin::Mutex;
use spin::MutexGuard;

const BLOCK_CACHE_SIZE: usize = 1 << 4;

pub struct BlockCacheInner {
    buf: Block,
    modified: bool,
}

impl BlockCacheInner {
    /// Safety:
    /// - Data at target offset must be valid for type T.
    unsafe fn read<T, V>(&self, offset: usize, f: impl FnOnce(&T) -> V) -> V {
        let type_size = core::mem::size_of::<T>();
        if offset + type_size > BLOCK_SIZE {
            panic!("out of bondary when trying to read block cache");
        }

        let ptr = &self.buf[offset] as *const u8 as *const T;
        let t = ptr.read_unaligned();
        let ret = f(&t);
        core::mem::forget(t);
        ret
    }

    /// Safety:
    /// - Data at target offset must be valid for type T.
    unsafe fn modify<T, V>(&mut self, offset: usize, f: impl FnOnce(&mut T) -> V) -> V {
        let type_size = core::mem::size_of::<T>();
        if offset + type_size > BLOCK_SIZE {
            panic!("out of bondary when trying to read block cache");
        }

        self.modified = true;
        let ptr = &mut self.buf[offset] as *mut u8 as *mut T;
        let mut t = ptr.read_unaligned();
        let ret = f(&mut t);
        ptr.write_unaligned(t);
        ret
    }
}

pub struct BlockCache {
    block_id: usize,
    inner: Mutex<BlockCacheInner>,
    block_dev: Arc<dyn BlockDevice>,
}

impl BlockCache {
    fn new(block_dev: Arc<dyn BlockDevice>, block_id: usize, block: Block) -> Self {
        let inner = BlockCacheInner {
            buf: block,
            modified: false,
        };
        Self {
            block_id,
            inner: Mutex::new(inner),
            block_dev,
        }
    }

    pub fn block_id(&self) -> usize {
        self.block_id
    }

    fn lock(&self) -> MutexGuard<BlockCacheInner> {
        self.inner.lock()
    }

    /// Safety:
    /// - Data at target offset must be valid for type T.
    pub unsafe fn read<T, V>(&self, offset: usize, f: impl FnOnce(&T) -> V) -> V {
        self.inner.lock().read(offset, f)
    }

    /// Safety:
    /// - Data at target offset must be valid for type T.
    pub unsafe fn modify<T, V>(&self, offset: usize, f: impl FnOnce(&mut T) -> V) -> V {
        self.inner.lock().modify(offset, f)
    }
}

impl Drop for BlockCache {
    fn drop(&mut self) {
        let inner = self.inner.get_mut();
        if inner.modified {
            self.block_dev.write_block(self.block_id, &inner.buf);
        }
    }

}

/// When out of cache slots, evict the unreferenced, first-cached entry.
pub struct BlockCacheManager {
    caches: VecDeque<Arc<BlockCache>>,
    block_dev: Arc<dyn BlockDevice>,
}

impl BlockCacheManager {
    pub fn new<T: BlockDevice + 'static>(dev: T) -> Self {
        BlockCacheManager { caches: VecDeque::new(), block_dev: Arc::new(dev) }
    }

    // Return &Arc to allow user to decide whether to clone it or not.
    fn put_cache(&mut self, cache: BlockCache) -> &Arc<BlockCache> {
        if self.caches.len() >= BLOCK_CACHE_SIZE {
            let evicted_pos = self.caches
                .iter()
                .position(|c| Arc::strong_count(c) == 1)
                .expect("out of block cache slots");

            let evicted = self.caches.remove(evicted_pos).unwrap();
            let inner = evicted.lock();
            if inner.modified {
                self.block_dev.write_block(evicted.block_id, &inner.buf);
            }
        }

        self.caches.push_back(Arc::new(cache));
        self.caches.back().unwrap()
    }

    fn get_cache(&mut self, block_id: usize) -> &Arc<BlockCache> {
        if let Some(idx) = self.caches
            .iter()
            .position(|b| b.block_id == block_id)
        {
            return &self.caches[idx];
        }
        let cache = {
            let mut buf: Block = [0; BLOCK_SIZE];
            self.block_dev.read_block(block_id, &mut buf);
            BlockCache::new(Arc::clone(&self.block_dev), block_id, buf)
        };
        self.put_cache(cache)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_dev::tests::TestBlockDevice;

    fn setup() -> (TestBlockDevice, BlockCacheManager) {
        let inner_dev = TestBlockDevice::new();
        let cache_mgr = BlockCacheManager::new(inner_dev.clone());
        (inner_dev, cache_mgr)
    }

    #[test]
    fn block_cache_mgr_basic() {
        let (inner_dev, mut cache_mgr) = setup();

        let mut buf = [0; BLOCK_SIZE];

        let b1 = Arc::clone(cache_mgr.get_cache(1));
        unsafe {
            b1.read(0, |d: &u8| {
                assert!(*d == 0);
            });

            b1.modify(0, |d: &mut u8| {
                *d = 1;
            });
            b1.read(0, |d: &u8| {
                assert!(*d == 1);
            });
        }
        let b2 = Arc::clone(cache_mgr.get_cache(2));
        unsafe {
            b2.modify(2, |d: &mut u8| {
                *d = 2;
            });
            b2.read(2, |d: &u8| {
                assert!(*d == 2);
            });
        }

        drop(b1);
        drop(b2);
        drop(cache_mgr);

        inner_dev.read_block(1, &mut buf);
        assert!(buf[0] == 1);
        inner_dev.read_block(2, &mut buf);
        assert!(buf[2] == 2);
    }

    #[test]
    fn block_cache_mgr_cache_size() {
        let (_, mut cache_mgr) = setup();
        for _ in 0..2 {
            cache_mgr.get_cache(0);
        }
        assert_eq!(cache_mgr.caches.len(), 1);
        for i in 0..BLOCK_CACHE_SIZE + 1 {
            cache_mgr.get_cache(i);
        }
        assert_eq!(cache_mgr.caches.len(), BLOCK_CACHE_SIZE);
    }
}