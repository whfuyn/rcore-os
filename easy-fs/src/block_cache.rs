use crate::Block;
use crate::BLOCK_SIZE;
use crate::block_dev::BlockDevice;
use alloc::sync::Arc;
use alloc::collections::VecDeque;
use spin::Mutex;
use spin::MutexGuard;
use core::mem::MaybeUninit;
use core::ptr::addr_of;
use core::ptr::addr_of_mut;

const BLOCK_CACHE_SIZE: usize = 1 << 4;

#[repr(C, align(16))]
pub struct BlockCacheInner {
    buf: Block,
    modified: bool,
}

impl BlockCacheInner {
    // TODO:
    // If the target ptr is properly aligned, we may avoid the copy in read_unaligned
    // and cast that ptr to reference directly.
    // How to make such a stituation as more as possible?

    /// Safety:
    /// - Data at target offset must be valid for type T.
    unsafe fn read<T, V>(&self, offset: usize, f: impl FnOnce(&T) -> V) -> V {
        self.read_maybe_uninit(offset, |r| {
            f(r.assume_init_ref())
        })
    }

    /// Safety:
    /// - Data at target offset must be valid for type T.
    unsafe fn modify<T, V>(&mut self, offset: usize, f: impl FnOnce(&mut T) -> V) -> V {
        self.modify_maybe_uninit(offset, |r| {
            f(r.assume_init_mut())
        })
    }

    fn read_maybe_uninit<T, V>(&self, offset: usize, f: impl FnOnce(&MaybeUninit<T>) -> V) -> V {
        let type_size = core::mem::size_of::<T>();
        if offset + type_size > BLOCK_SIZE {
            panic!("out of boundary when trying to read block cache");
        }

        let ptr = addr_of!(self.buf[offset]) as *const MaybeUninit<T>;
        let t = unsafe { ptr.read_unaligned() };
        f(&t)
    }

    fn modify_maybe_uninit<T, V>(&mut self, offset: usize, f: impl FnOnce(&mut MaybeUninit<T>) -> V) -> V {
        let type_size = core::mem::size_of::<T>();
        if offset + type_size > BLOCK_SIZE {
            panic!("out of boundary when trying to modify block cache");
        }

        self.modified = true;
        let ptr = addr_of_mut!(self.buf[offset]) as *mut MaybeUninit<T>;
        unsafe {
            let mut t = ptr.read_unaligned();
            let ret = f(&mut t);
            ptr.write_unaligned(t);
            ret
        }
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

    pub fn lock(&self) -> MutexGuard<BlockCacheInner> {
        self.inner.lock()
    }

    pub fn flush(&self) {
        let mut inner = self.lock();
        if inner.modified {
            self.block_dev.write_block(self.block_id, &inner.buf);
            inner.modified = false;
        }
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

    pub fn read_maybe_uninit<T, V>(&self, offset: usize, f: impl FnOnce(&MaybeUninit<T>) -> V) -> V {
        self.inner.lock().read_maybe_uninit(offset, f)
    }

    pub fn modify_maybe_uninit<T, V>(&self, offset: usize, f: impl FnOnce(&mut MaybeUninit<T>) -> V) -> V {
        self.inner.lock().modify_maybe_uninit(offset, f)
    }
}

impl Drop for BlockCache {
    fn drop(&mut self) {
        self.flush();
    }
}

/// When out of cache slots, evict the unreferenced, first-cached entry.
/// i.e. FIFO with ref count limit.
pub struct BlockCacheManager {
    caches: VecDeque<Arc<BlockCache>>,
    block_dev: Arc<dyn BlockDevice>,
}

impl BlockCacheManager {
    pub fn new<T: BlockDevice>(block_dev: T) -> Self {
        BlockCacheManager { caches: VecDeque::new(), block_dev: Arc::new(block_dev) }
    }

    // Return &Arc to allow user to decide whether to clone it or not.
    fn put_block(&mut self, cache: BlockCache) -> &Arc<BlockCache> {
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

    pub fn get_block(&mut self, block_id: usize) -> &Arc<BlockCache> {
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
        self.put_block(cache)
    }

    pub fn flush(&self)  {
        self.caches.iter().for_each(|c| c.flush());
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::block_dev::tests::TestBlockDevice;

    pub fn setup() -> (TestBlockDevice, BlockCacheManager) {
        let inner_dev = TestBlockDevice::new();
        let cache_mgr = BlockCacheManager::new(inner_dev.clone());
        (inner_dev, cache_mgr)
    }

    #[test]
    fn block_cache_mgr_basic() {
        let (inner_dev, mut cache_mgr) = setup();

        let mut buf = [0; BLOCK_SIZE];

        let b1 = Arc::clone(cache_mgr.get_block(1));
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
        let b2 = Arc::clone(cache_mgr.get_block(2));
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
            cache_mgr.get_block(0);
        }
        assert_eq!(cache_mgr.caches.len(), 1);
        for i in 0..BLOCK_CACHE_SIZE + 1 {
            cache_mgr.get_block(i);
        }
        assert_eq!(cache_mgr.caches.len(), BLOCK_CACHE_SIZE);
    }
}
