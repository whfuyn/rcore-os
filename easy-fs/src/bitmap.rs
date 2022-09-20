use crate::{
    block_dev::BlockDevice,
    BLOCK_SIZE,
    block_cache::get_block_cache,
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

    pub fn alloc(&self) -> Option<u32> {
        for block_pos in 0..self.size {
            let block_id = self.start_block + block_pos;
            let block = get_block_cache(block_id);
            let f = |bitmap: &mut BitmapBlock| {
                bitmap
                    .iter_mut()
                    .enumerate()
                    .find_map(|(u64_pos, b)|
                        if *b != u64::MAX {
                            let bit_pos = b.trailing_ones();
                            // *b |= *b + 1;
                            *b |= 1 << bit_pos;
                            Some((u64_pos as u32) * u64::BITS + bit_pos)
                        } else {
                            None
                        }
                    )
            };
            if let Some(inner_pos) = unsafe { block.modify(0, f) } {
                return Some((block_pos * BLOCK_BITS) as u32 + inner_pos);
            }
        }
        None
    }

    pub fn dealloc(&self, block_offset: u32) {
        let bit_pos = block_offset % u64::BITS;
        let u64_pos = block_offset % BLOCK_BITS as u32 / 64;
        let block_pos = block_offset / BLOCK_BITS as u32;

        if block_pos >= self.size as u32 {
            panic!("try to dealloc a block offset that is out of the bitmap");
        }
        let block_id = self.start_block + block_pos as usize;
        let block = get_block_cache(block_id);
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
    use crate::block_cache::init_block_cache;
    use crate::block_cache::flush_block_cache;
    use crate::block_dev::tests::TestBlockDevice;

    #[test]
    fn bitmap_basic() {
        let block_dev = TestBlockDevice::new();
        init_block_cache(block_dev.clone());

        let bitmap = Bitmap::new(0, 1);
        for i in 0..128 {
            assert_eq!(bitmap.alloc(), Some(i));
        }

        flush_block_cache();

        let mut buf = [0u8; BLOCK_SIZE];
        block_dev.read_block(0, &mut buf);
        assert_eq!(u64::from_le_bytes(buf[0..8].try_into().unwrap()), u64::MAX);
        block_dev.read_block(0, &mut buf);
        assert_eq!(u64::from_le_bytes(buf[8..16].try_into().unwrap()), u64::MAX);

        bitmap.dealloc(0);
        bitmap.dealloc(127);
        assert_eq!(bitmap.alloc(), Some(0));
        assert_eq!(bitmap.alloc(), Some(127));
        bitmap.dealloc(64);
        flush_block_cache();

        block_dev.read_block(0, &mut buf);
        assert_eq!(u64::from_le_bytes(buf[8..16].try_into().unwrap()), u64::MAX - 1);
    }
}

