use crate::Block;
use crate::BLOCK_SIZE;
use crate::block_dev::BlockDevice;
use alloc::boxed::Box;
use alloc::collections::VecDeque;

const BLOCK_CACHE_SIZE: usize = 1 << 4;

pub struct BlockCache {
    cache: Block,
    block_id: usize,

    modified: bool,
}

impl BlockCache {
    fn new(block_id: usize, block: Block) -> Self {
        BlockCache {
            cache: block,
            block_id,
            modified: false,
        }
    }
}

// FIFO cache
pub struct BlockCacheManager {
    caches: VecDeque<BlockCache>,
    block_dev: Box<dyn BlockDevice>,
}

impl BlockCacheManager {
    pub fn new<T: BlockDevice + 'static>(dev: T) -> Self {
        BlockCacheManager { caches: VecDeque::new(), block_dev: Box::new(dev) }
    }

    fn put_cache(&mut self, cache: BlockCache) -> &mut BlockCache {
        let evicted = self.caches.pop_front();

        if let Some(evicted) = evicted {
            if evicted.modified {
                self.block_dev.write_block(evicted.block_id, &evicted.cache);
            }
        }

        self.caches.push_back(cache);
        self.caches.back_mut().unwrap()
    }

    fn get_cache(&mut self, block_id: usize) -> &mut BlockCache {
        if let Some(idx) = self.caches
            .iter()
            .position(|b| b.block_id == block_id)
        {
            return &mut self.caches[idx];
        }
        let cache = {
            let mut buf: Block = [0; BLOCK_SIZE];
            self.block_dev.read_block(block_id, &mut buf);
            BlockCache {
                cache: buf,
                block_id,
                modified: false,
            }
        };
        self.put_cache(cache)
    }
}

impl Drop for BlockCacheManager {
    fn drop(&mut self) {
        for cache in self.caches.drain(..) {
            if cache.modified {
                self.block_dev.write_block(cache.block_id, &cache.cache);
            }
        }
    }
}

impl BlockDevice for BlockCacheManager {
    fn read_block(&mut self, block_id: usize, buf: &mut Block) {
        let cache = self.get_cache(block_id);
        buf.copy_from_slice(&cache.cache);
    }

    fn write_block(&mut self, block_id: usize, buf: &Block) {
        let cache = self.get_cache(block_id);
        cache.cache.copy_from_slice(buf);
        cache.modified = true;
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_dev::tests::TestBlockDevice;

    #[test]
    fn block_cache_basic() {
        let mut inner_dev = TestBlockDevice::new();
        let mut cache_mgr = BlockCacheManager::new(inner_dev.clone());

        let b1 = [6; BLOCK_SIZE];
        let mut buf = [0; BLOCK_SIZE];

        cache_mgr.read_block(233, &mut buf);
        assert!(buf.iter().all(|&b| b == 0));

        cache_mgr.write_block(233, &b1);
        cache_mgr.read_block(233, &mut buf);
        assert!(buf.iter().all(|&b| b == 6));

        cache_mgr.write_block(666, &b1);
        cache_mgr.read_block(666, &mut buf);
        assert!(buf.iter().all(|&b| b == 6));

        drop(cache_mgr);

        inner_dev.read_block(0, &mut buf);
        assert!(buf.iter().all(|&b| b == 0));
        inner_dev.read_block(233, &mut buf);
        assert!(buf.iter().all(|&b| b == 6));
        inner_dev.read_block(666, &mut buf);
        assert!(buf.iter().all(|&b| b == 6));
    }
}
