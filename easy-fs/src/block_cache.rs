use crate::Block;
use crate::BLOCK_SIZE;
use crate::block_dev::BlockDevice;
use alloc::sync::Arc;
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
    block_dev: Arc<dyn BlockDevice>,
}

impl BlockCacheManager {
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

    pub fn get_block(&mut self, block_id: usize) -> &Block {
        &*self.get_block_mut(block_id)
    }

    pub fn get_block_mut(&mut self, block_id: usize) -> &mut Block {
        if let Some(idx) = self.caches
            .iter()
            .position(|b| b.block_id == block_id)
        {
            return &mut self.caches[idx].cache;
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
        &mut self.put_cache(cache).cache
    }
}
