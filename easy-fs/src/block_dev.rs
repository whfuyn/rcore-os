use crate::Block;

pub trait BlockDevice: Send + Sync + 'static {
    fn read_block(&self, block_id: usize, buf: &mut Block);
    fn write_block(&self, block_id: usize, buf: &Block);
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::BLOCK_SIZE;
    use alloc::collections::BTreeMap;
    use alloc::sync::Arc;
    use spin::Mutex;

    #[derive(Clone)]
    pub struct TestBlockDevice {
        blocks: Arc<Mutex<BTreeMap<usize, Block>>>,
    }

    impl TestBlockDevice {
        pub fn new() -> Self {
            Self {
                blocks: Arc::new(Mutex::new(BTreeMap::new())),
            }
        }
    }

    impl BlockDevice for TestBlockDevice {
        fn read_block(&self, block_id: usize, buf: &mut Block) {
            let mut blocks = self.blocks.lock();
            let block = blocks.entry(block_id).or_insert_with(|| [0; BLOCK_SIZE]);
            buf.copy_from_slice(block);
        }

        fn write_block(&self, block_id: usize, buf: &Block) {
            self.blocks.lock().insert(block_id, *buf);
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
