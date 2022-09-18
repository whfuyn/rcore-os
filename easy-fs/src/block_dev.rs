use crate::Block;

pub trait BlockDevice {
    fn read_block(&self, block_id: usize, buf: &mut Block);
    fn write_block(&self, block_id: usize, buf: &Block);
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::BLOCK_SIZE;
    use alloc::collections::BTreeMap;
    use core::cell::RefCell;

    pub struct TestBlockDevice {
        blocks: RefCell<BTreeMap<usize, Block>>,
    }

    impl TestBlockDevice {
        fn new() -> Self {
            Self { blocks: RefCell::new(BTreeMap::new()) }
        }
    }

    impl BlockDevice for TestBlockDevice {
        fn read_block(&self, block_id: usize, buf: &mut Block) {
            let mut blocks = self.blocks.borrow_mut();
            let block = blocks
                .entry(block_id)
                .or_insert_with(|| [0; BLOCK_SIZE]);
            buf.copy_from_slice(block);
        }

        fn write_block(&self, block_id: usize, buf: &Block) {
            let mut blocks = self.blocks.borrow_mut();
            blocks.insert(block_id, buf.clone());
        }
    }

    #[test]
    fn test_block_device_basic() {
        let dev = TestBlockDevice::new();
        let b1 = [6; BLOCK_SIZE];
        let mut buf = [0; BLOCK_SIZE];

        dev.read_block(233, &mut buf);
        assert!(buf.iter().all(|&b| b == 0));

        dev.write_block(233, &b1);
        dev.read_block(233, &mut buf);
        assert!(buf.iter().all(|&b| b == 6));

        dev.write_block(666, &b1);
        dev.read_block(666, &mut buf);
        assert!(buf.iter().all(|&b| b == 6));
    }
}

