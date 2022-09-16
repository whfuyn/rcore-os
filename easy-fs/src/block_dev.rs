use crate::Block;

pub trait BlockDevice {
    fn read_block(&self, block_id: usize, buf: &mut Block);
    fn write_block(&self, block_id: usize, buf: &Block);
}
