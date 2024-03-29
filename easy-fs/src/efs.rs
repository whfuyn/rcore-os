use crate::bitmap::Bitmap;
use crate::block_cache::BlockCache;
use crate::block_cache::BlockCacheManager;
use crate::layout::*;
use crate::BLOCK_SIZE;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::mem;
use core::mem::MaybeUninit;
use spin::Mutex;

const DISK_INODE_SIZE: usize = mem::size_of::<DiskInode>();
const DISK_INODES_IN_BLOCK: usize = BLOCK_SIZE / DISK_INODE_SIZE;

pub(crate) struct Inode {
    id: u32,
    fs: Arc<EasyFileSystem>,
}

impl Inode {
    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn ty(&self) -> InodeType {
        self.read_disk_inode(|di, _| di.ty)
    }

    pub fn size(&self) -> usize {
        self.read_disk_inode(|di, _| di.size as usize)
    }

    pub fn resize(&self, new_size: u32) {
        self.modify_disk_inode(|di, fs| di.resize(new_size, fs));
    }

    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        self.read_disk_inode(|di, fs| di.read_at(offset, buf, fs))
    }

    pub fn write_at(&self, offset: usize, data: &[u8]) {
        self.modify_disk_inode(|di, fs| di.write_at(offset, data, fs));
    }

    pub fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode, &Arc<EasyFileSystem>) -> V) -> V {
        let g = |di: &DiskInode| f(di, &self.fs);
        self.fs.read_disk_inode(self.id, g)
    }

    pub fn modify_disk_inode<V>(
        &self,
        f: impl FnOnce(&mut DiskInode, &Arc<EasyFileSystem>) -> V,
    ) -> V {
        let g = |di: &mut DiskInode| f(di, &self.fs);
        self.fs.modify_disk_inode(self.id, g)
    }

    pub fn fs(&self) -> &Arc<EasyFileSystem> {
        &self.fs
    }
}

impl Drop for Inode {
    fn drop(&mut self) {
        self.fs.close_inode(self.id);
    }
}

struct OpenInodeRecord {
    ref_count: usize,
    pending_delete: bool,
}

pub struct EasyFileSystem {
    inode_area_start: usize,
    inode_bitmap: Bitmap,
    data_area_start: usize,
    data_bitmap: Bitmap,

    open_inodes: Mutex<BTreeMap<u32, OpenInodeRecord>>,
    cache_mgr: Arc<Mutex<BlockCacheManager>>,
}

impl EasyFileSystem {
    fn new(
        cache_mgr: BlockCacheManager,
        inode_bitmap_blocks: u32,
        inode_area_blocks: u32,
        data_bitmap_blocks: u32,
        data_area_blocks: u32,
    ) -> Arc<Self> {
        let cache_mgr = Arc::new(Mutex::new(cache_mgr));

        let inode_bitmap_start = 1;
        let inode_bitmap = Bitmap::new(
            inode_bitmap_start as usize,
            inode_bitmap_blocks as usize,
            inode_area_blocks as usize,
        );
        let inode_area_start = inode_bitmap_start + inode_bitmap_blocks;

        let data_bitmap_start = inode_area_start + inode_area_blocks;
        let data_bitmap = Bitmap::new(
            data_bitmap_start as usize,
            data_bitmap_blocks as usize,
            data_area_blocks as usize,
        );
        let data_area_start = data_bitmap_start + data_bitmap_blocks;
        let efs = Self {
            inode_area_start: inode_area_start as usize,
            inode_bitmap,
            data_area_start: data_area_start as usize,
            data_bitmap,
            open_inodes: Mutex::new(BTreeMap::new()),
            cache_mgr,
        };
        Arc::new(efs)
    }

    pub fn create(
        mut cache_mgr: BlockCacheManager,
        total_blocks: u32,
        inode_bitmap_blocks: u32,
        inode_area_blocks: u32,
        data_bitmap_blocks: u32,
        data_area_blocks: u32,
    ) -> Arc<Self> {
        let super_block_cache = Arc::clone(cache_mgr.get_block(0));
        let f = |b: &mut MaybeUninit<SuperBlock>| {
            b.write(SuperBlock::new(
                total_blocks,
                inode_bitmap_blocks,
                inode_area_blocks,
                data_bitmap_blocks,
                data_area_blocks
            ));
        };
        super_block_cache.modify_maybe_uninit(0, f);

        Self::new(
            cache_mgr,
            inode_bitmap_blocks,
            inode_area_blocks,
            data_bitmap_blocks,
            data_area_blocks
        )
    }

    pub fn open(
        mut cache_mgr: BlockCacheManager,
    ) -> Result<Arc<Self>, BlockCacheManager> {
        let super_block_cache = cache_mgr.get_block(0);
        let f = |b: &SuperBlock| *b;
        let sblk = unsafe { super_block_cache.read(0, f) };
        if sblk.validate() {
            Ok(Self::new(
                cache_mgr,
                sblk.inode_bitmap_blocks,
                sblk.inode_area_blocks,
                sblk.data_bitmap_blocks,
                sblk.data_area_blocks
            ))
        } else {
            Err(cache_mgr)
        }
    }

    pub(crate) fn alloc_inode(self: &Arc<Self>, ty: InodeType) -> Option<Inode> {
        let mut open_inodes = self.open_inodes.lock();
        let mut cache_mgr = self.cache_mgr.lock();

        let inode_id = self.inode_bitmap.alloc(&mut cache_mgr)?;
        let inode_block_id = self.inode_area_start + inode_id / DISK_INODES_IN_BLOCK;
        let inode_offset = inode_id % DISK_INODES_IN_BLOCK * DISK_INODE_SIZE;

        let inode_block = cache_mgr.get_block(inode_block_id);
        let f = |di: &mut MaybeUninit<DiskInode>| {
            di.write(DiskInode::new(ty));
        };
        inode_block.modify_maybe_uninit(inode_offset, f);
        assert!(open_inodes
            .insert(
                inode_id as u32,
                OpenInodeRecord {
                    ref_count: 1,
                    pending_delete: false
                }
            )
            .is_none());

        Inode {
            id: inode_id as u32,
            fs: Arc::clone(self),
        }
        .into()
    }

    fn dealloc_inode(&self, inode_id: u32) {
        let f = |di: &mut DiskInode| di.resize(0, self);
        self.modify_disk_inode(inode_id, f);

        self.inode_bitmap
            .dealloc(inode_id as usize, &mut self.cache_mgr.lock());
    }

    pub(crate) fn get_block(&self, block_id: usize) -> Arc<BlockCache> {
        let mut cache_mgr = self.cache_mgr.lock();
        let slot = block_id - self.data_area_start;
        if !self.data_bitmap.is_allocated(slot, &mut cache_mgr) {
            panic!("block `{}` isn't allocated", block_id);
        }

        Arc::clone(cache_mgr.get_block(block_id))
    }

    pub(crate) fn alloc_block(&self) -> Option<Arc<BlockCache>> {
        let mut cache_mgr = self.cache_mgr.lock();
        let block_id = self.data_area_start + self.data_bitmap.alloc(&mut cache_mgr)?;
        Some(Arc::clone(cache_mgr.get_block(block_id)))
    }

    pub(crate) fn dealloc_block(&self, block_id: usize) {
        let slot = block_id - self.data_area_start;
        self.data_bitmap.dealloc(slot, &mut self.cache_mgr.lock());
    }

    pub(crate) fn open_inode(self: &Arc<Self>, inode_id: u32) -> Option<Inode> {
        let mut open_inodes = self.open_inodes.lock();

        use alloc::collections::btree_map::Entry;
        match open_inodes.entry(inode_id) {
            Entry::Occupied(mut occupied) => {
                let record = occupied.get_mut();
                if record.pending_delete {
                    return None;
                }
                record.ref_count += 1;
            }
            Entry::Vacant(vacant) => {
                if !self
                    .inode_bitmap
                    .is_allocated(inode_id as usize, &mut self.cache_mgr.lock())
                {
                    return None;
                }
                vacant.insert(OpenInodeRecord {
                    ref_count: 1,
                    pending_delete: false,
                });
            }
        }

        Inode {
            id: inode_id,
            fs: Arc::clone(self),
        }
        .into()
    }

    fn close_inode(&self, inode_id: u32) {
        let mut open_inodes = self.open_inodes.lock();
        let record = open_inodes
            .get_mut(&inode_id)
            .expect("try to close a file that isn't opened");
        record.ref_count -= 1;
        if record.ref_count == 0 && record.pending_delete {
            // Release lock when we are reclaiming the space used by the deleted inode.
            // Open inode won't be able to open the deleted, because of the pending_delete flag.
            drop(open_inodes);
            self.dealloc_inode(inode_id);
            self.open_inodes.lock().remove(&inode_id);
        }
    }

    pub(crate) fn delete_inode(&self, inode_id: u32) {
        let mut open_inodes = self.open_inodes.lock();
        // Doing the check with the open_inodes lock to avoid double-free.
        if !self
            .inode_bitmap
            .is_allocated(inode_id as usize, &mut self.cache_mgr.lock())
        {
            return;
        }
        // Insert a delete record to avoid opening the deleted node.
        let record = open_inodes.entry(inode_id).or_insert(OpenInodeRecord {
            ref_count: 0,
            pending_delete: true,
        });
        record.pending_delete = true;
        if record.ref_count == 0 {
            drop(open_inodes);
            self.dealloc_inode(inode_id);
            self.open_inodes.lock().remove(&inode_id);
        }
    }

    fn get_disk_inode_index(&self, inode_id: u32) -> (usize, usize) {
        let inode_id = inode_id as usize;
        let di_block_id = self.inode_area_start + inode_id / DISK_INODES_IN_BLOCK;
        let di_offset = inode_id % DISK_INODES_IN_BLOCK * DISK_INODE_SIZE;
        (di_block_id, di_offset)
    }

    fn read_disk_inode<V>(&self, inode_id: u32, f: impl FnOnce(&DiskInode) -> V) -> V {
        let mut cache_mgr = self.cache_mgr.lock();
        assert!(self
            .inode_bitmap
            .is_allocated(inode_id as usize, &mut cache_mgr));

        let (block_id, offset) = self.get_disk_inode_index(inode_id);
        let block = Arc::clone(cache_mgr.get_block(block_id));
        drop(cache_mgr);
        unsafe { block.read(offset, f) }
    }

    fn modify_disk_inode<V>(&self, inode_id: u32, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        let mut cache_mgr = self.cache_mgr.lock();
        assert!(self
            .inode_bitmap
            .is_allocated(inode_id as usize, &mut cache_mgr));

        let (block_id, offset) = self.get_disk_inode_index(inode_id);
        let block = Arc::clone(cache_mgr.get_block(block_id));
        drop(cache_mgr);
        unsafe { block.modify(offset, f) }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::block_cache::tests::setup as block_cache_setup;
    use crate::BLOCK_BITS;

    pub fn setup() -> Arc<EasyFileSystem> {
        let (_block_dev, cache_mgr) = block_cache_setup();
        EasyFileSystem::create(
            cache_mgr,
            1 + 2 * BLOCK_BITS as u32 + 2,
            1,
            BLOCK_BITS as u32,
            1,
            BLOCK_BITS as u32,
        )
    }

    // #[test]
    // fn inode_basic() {
    //     let (_block_dev, mut cache_mgr) = setup();
    //     let bitmap = Bitmap::new(0, 10);
    //     let block_allocator = BlockAllocator::new(10, &bitmap);

    //     let inode_block = block_allocator.alloc(&mut cache_mgr).unwrap();
    //     let inode = Inode::create_file(inode_block.block_id(), 0, &mut cache_mgr);

    //     let data = b"hello, world!";
    //     let mut buf = [0; 13];
    //     unsafe {
    //         inode.write_at(510, data, &block_allocator, &mut cache_mgr);
    //         inode.read_at(510, &mut buf, &mut cache_mgr);
    //         assert_eq!(data, &buf);
    //         assert_eq!(inode.size(&mut cache_mgr), 523);

    //         inode.resize(888, &block_allocator, &mut cache_mgr);
    //         assert_eq!(inode.size(&mut cache_mgr), 888);
    //     }
    // }

    // #[test]
    // fn inode_read_over_bound() {
    //     let (_block_dev, mut cache_mgr) = setup();
    //     let bitmap = Bitmap::new(0, 10);
    //     let block_allocator = BlockAllocator::new(10, &bitmap);

    //     let inode_block = block_allocator.alloc(&mut cache_mgr).unwrap();
    //     let inode = Inode::create_file(inode_block.block_id(), 0, &mut cache_mgr);

    //     let data = b"hello, world!";
    //     let mut buf = [0; 13];
    //     unsafe {
    //         inode.write_at(0, data, &block_allocator, &mut cache_mgr);
    //         assert_eq!(inode.read_at(13, &mut buf, &mut cache_mgr), 0);
    //         assert_eq!(inode.read_at(1, &mut buf, &mut cache_mgr), 12);
    //     }
    //     assert_eq!(&buf[..12], &data[1..]);
    // }
}
