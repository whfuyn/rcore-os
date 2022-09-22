use crate::{
    block_dev::BlockDevice,
    BLOCK_SIZE,
    block_cache::BlockCacheManager,

};


const BLOCK_BITS: usize = BLOCK_SIZE * 8;

type BitmapBlock = [u64; 64];

pub struct Bitmap {
    start_block: usize,
    size: usize,
}

impl Bitmap {
    pub fn new(start_block: usize, size: usize) -> Self {
        Self {
            start_block,
            size,
        }
    }

    pub fn alloc(&self, cache_mgr: &mut BlockCacheManager) -> Option<usize> {
        for block_pos in 0..self.size {
            let block_id = self.start_block + block_pos;
            let block = cache_mgr.get_block(block_id);
            let f = |bitmap: &mut BitmapBlock| {
                bitmap
                    .iter_mut()
                    .enumerate()
                    .find_map(|(u64_pos, b)|
                        if *b != u64::MAX {
                            let bit_pos = b.trailing_ones() as usize;
                            // *b |= *b + 1;
                            *b |= 1 << bit_pos;
                            Some(u64_pos * u64::BITS as usize + bit_pos)
                        } else {
                            None
                        }
                    )
            };
            if let Some(inner_pos) = unsafe { block.modify(0, f) } {
                return Some(self.start_block + (block_pos * BLOCK_BITS) + inner_pos as usize);
            }
        }
        None
    }

    pub fn dealloc(&self, block_id: usize, cache_mgr: &mut BlockCacheManager) {
        if block_id == 0 {
            return;
        }

        let block_offset = block_id - self.start_block;
        let bit_pos = block_offset % u64::BITS as usize;
        let u64_pos = block_offset % BLOCK_BITS / 64;
        let block_pos = block_offset / BLOCK_BITS;

        if block_pos >= self.size {
            panic!("try to dealloc a block offset that is out of the bitmap");
        }
        let block = cache_mgr.get_block(block_id);
        let f = |bitmap: &mut BitmapBlock| {
            assert!(bitmap[u64_pos as usize] & (1 << bit_pos) != 0);
            bitmap[u64_pos as usize] &= !(1 << bit_pos);
        };
        unsafe {
            block.modify(0, f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_cache::tests::setup;

    #[test]
    fn bitmap_basic() {
        let (block_dev, mut cache_mgr) = setup();

        let bitmap = Bitmap::new(0, 1);
        for i in 0..128 {
            assert_eq!(bitmap.alloc(&mut cache_mgr), Some(i));
        }

        flush_block_cache();

        let mut buf = [0u8; BLOCK_SIZE];
        block_dev.read_block(0, &mut buf);
        assert_eq!(u64::from_le_bytes(buf[0..8].try_into().unwrap()), u64::MAX);
        block_dev.read_block(0, &mut buf);
        assert_eq!(u64::from_le_bytes(buf[8..16].try_into().unwrap()), u64::MAX);

        bitmap.dealloc(0, &mut cache_mgr);
        bitmap.dealloc(127, &mut cache_mgr);
        assert_eq!(bitmap.alloc(&mut cache_mgr), Some(0));
        assert_eq!(bitmap.alloc(&mut cache_mgr), Some(127));
        bitmap.dealloc(64, &mut cache_mgr);
        flush_block_cache();

        block_dev.read_block(0, &mut buf);
        assert_eq!(u64::from_le_bytes(buf[8..16].try_into().unwrap()), u64::MAX - 1);
    }
}

