use crate::{
    block_dev::BlockDevice,
    BLOCK_SIZE,
};


const BLOCK_BITS: usize = BLOCK_SIZE * 8;

type BitmapBlock = [u64; 64];

pub struct Bitmap {
    start_block: usize,
    size: usize,
}

impl Bitmap {
    pub fn alloc(&self, block_dev: &dyn BlockDevice) -> Option<u32> {
        todo!()
    }
}

