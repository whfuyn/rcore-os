use crate::Block;

pub trait BlockDevice {
    fn read_block(&mut self, block_id: usize, buf: &mut Block);
    fn write_block(&mut self, block_id: usize, buf: &Block);
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::BLOCK_SIZE;
    use alloc::collections::BTreeMap;
    use alloc::rc::Rc;
    use core::cell::RefCell;

    #[derive(Clone)]
    pub struct TestBlockDevice {
        blocks: Rc<RefCell<BTreeMap<usize, Block>>>,
    }

    impl TestBlockDevice {
        pub fn new() -> Self {
            Self { blocks: Rc::new(RefCell::new(BTreeMap::new())) }
        }
    }

    impl BlockDevice for TestBlockDevice {
        fn read_block(&mut self, block_id: usize, buf: &mut Block) {
            let mut blocks = self.blocks.borrow_mut();
            let block = blocks
                .entry(block_id)
                .or_insert_with(|| [0; BLOCK_SIZE]);
            buf.copy_from_slice(block);
        }

        fn write_block(&mut self, block_id: usize, buf: &Block) {
            self.blocks.borrow_mut().insert(block_id, buf.clone());
        }
    }

    #[test]
    fn test_block_device_basic() {
        let mut dev = TestBlockDevice::new();
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

