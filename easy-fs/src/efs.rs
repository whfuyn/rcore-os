use core::mem;
use core::mem::MaybeUninit;
use crate::BLOCK_SIZE;
use crate::layout::SuperBlock;
use crate::layout::DiskInode;
use crate::layout::InodeType;
use crate::bitmap::Bitmap;
use crate::block_dev::BlockDevice;
use crate::block_cache::BlockCache;
use crate::block_cache::BlockCacheManager;
use spin::Mutex;
use alloc::sync::Arc;


const DISK_INODE_SIZE: usize = mem::size_of::<DiskInode>();
const DISK_INODES_IN_BLOCK: usize = BLOCK_SIZE / DISK_INODE_SIZE;


pub struct Inode {
    block_id: usize,
    offset: usize,
    fs: Arc<EasyFileSystem>,
}

impl Inode {
    pub unsafe fn size(&self, fs: &EasyFileSystem) -> usize {
        let block = fs.get_block(self.block_id);
        let f = |di: &DiskInode| {
            di.size as usize
        };
        block.read(self.offset, f)
    }

    pub unsafe fn resize(&self, new_size: u32, fs: &EasyFileSystem) {
        let block = fs.get_block(self.block_id);
        let f = |di: &mut DiskInode| {
            di.resize(new_size, fs);
        };
        block.modify(self.offset, f);
    }

    pub unsafe fn read_at(&self, offset: usize, buf: &mut [u8], fs: &EasyFileSystem) -> usize {
        let block = fs.get_block(self.block_id);
        let f = |di: &DiskInode| {
            di.read_at(offset, buf, fs)
        };
        block.read(self.offset, f)
    }

    pub unsafe fn write_at(&self, offset: usize, data: &[u8], fs: &EasyFileSystem) {
        let block = fs.get_block(self.block_id);
        let f = |di: &mut DiskInode| {
            di.write_at(offset, data, fs)
        };
        block.modify(self.offset, f);
    }
}

pub struct EasyFileSystem {
    inode_area_start: usize,
    inode_bitmap: Bitmap,
    data_area_start: usize,
    data_bitmap: Bitmap,

    cache_mgr: Mutex<BlockCacheManager>,
    // block_dev: Arc<dyn BlockDevice>,
}

impl EasyFileSystem {
    fn create() -> Self {
        todo!()
    }

    fn load() -> Self {
        todo!()
    }

    pub fn get_block(&self, block_id: usize) -> Arc<BlockCache> {
        let mut cache_mgr = self.cache_mgr.lock();
        Arc::clone(cache_mgr.get_block(block_id))
    }

    fn get_inode(self: &Arc<Self>, inode_id: usize) -> Option<Inode> {
        if !self.inode_bitmap.is_allocated(inode_id, self) {
            return None;
        }

        let block_id = self.inode_area_start + inode_id / DISK_INODES_IN_BLOCK;
        let offset = inode_id % DISK_INODES_IN_BLOCK * DISK_INODE_SIZE;
        Inode {
            block_id,
            offset,
            fs: Arc::clone(self),
        }.into()
    }

    fn alloc_inode(self: &Arc<Self>, ty: InodeType) -> Option<Inode> {
        let inode_id = self.inode_bitmap.alloc(self)?;
        let inode_block_id = self.inode_area_start + inode_id / DISK_INODES_IN_BLOCK;
        let inode_offset = inode_id % DISK_INODES_IN_BLOCK * DISK_INODE_SIZE;

        let inode_block = self.get_block(inode_block_id);
        let f = |di: &mut MaybeUninit<DiskInode>| {
            di.write(DiskInode::new(ty));
        };
        inode_block.modify_maybe_uninit(inode_offset, f);

        Inode {
            block_id: inode_block_id,
            offset: inode_offset,
            fs: Arc::clone(self),
        }.into()
    }

    pub fn alloc_block(&self) -> Option<Arc<BlockCache>> {
        let block_id = self.data_area_start + self.data_bitmap.alloc(self)?;
        self.get_block(block_id).into()
    }

    pub fn dealloc_block(&self, block_id: usize) {
        let slot = block_id - self.data_area_start;
        self.data_bitmap.dealloc(slot, self);
    }

    fn get_root_inode() {

    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_cache::tests::setup;

    #[test]
    fn inode_basic() {
        let (_block_dev, mut cache_mgr) = setup();
        let bitmap = Bitmap::new(0, 10);
        let block_allocator = BlockAllocator::new(10, &bitmap);

        let inode_block = block_allocator.alloc(&mut cache_mgr).unwrap();
        let inode = Inode::create_file(inode_block.block_id(), 0, &mut cache_mgr);

        let data = b"hello, world!";
        let mut buf = [0; 13];
        unsafe {
            inode.write_at(510, data, &block_allocator, &mut cache_mgr);
            inode.read_at(510, &mut buf, &mut cache_mgr);
            assert_eq!(data, &buf);
            assert_eq!(inode.size(&mut cache_mgr), 523);

            inode.resize(888, &block_allocator, &mut cache_mgr);
            assert_eq!(inode.size(&mut cache_mgr), 888);
        }
    }

    #[test]
    fn inode_read_over_bound() {
        let (_block_dev, mut cache_mgr) = setup();
        let bitmap = Bitmap::new(0, 10);
        let block_allocator = BlockAllocator::new(10, &bitmap);

        let inode_block = block_allocator.alloc(&mut cache_mgr).unwrap();
        let inode = Inode::create_file(inode_block.block_id(), 0, &mut cache_mgr);

        let data = b"hello, world!";
        let mut buf = [0; 13];
        unsafe {
            inode.write_at(0, data, &block_allocator, &mut cache_mgr);
            assert_eq!(inode.read_at(13, &mut buf, &mut cache_mgr), 0);
            assert_eq!(inode.read_at(1, &mut buf, &mut cache_mgr), 12);
        }
        assert_eq!(&buf[..12], &data[1..]);
    }

}