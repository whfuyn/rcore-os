use alloc::sync::Arc;

use core::mem;
use crate::{BLOCK_SIZE, Block};
use crate::block_cache::BlockCacheManager;
use crate::block_cache::BlockCache;
use crate::bitmap::Bitmap;
use core::mem::MaybeUninit;

const EASY_FS_MAGIC: u32 = 0x666;
const INODE_DIRECT_COUNT: usize = 28;
const INODE_INDIRECT_COUNT: usize = BLOCK_SIZE / core::mem::size_of::<u32>();

const MAX_FILE_SIZE: usize = (INODE_DIRECT_COUNT + INODE_INDIRECT_COUNT + INODE_INDIRECT_COUNT.pow(2)) * BLOCK_SIZE;

type IndirectBlock = [u32; INODE_INDIRECT_COUNT];

#[repr(C)]
pub struct SuperBlock {
    magic: u32,
    pub total_blocks: u32,
    pub inode_bitmap_blocks: u32,
    pub inode_area_blocks: u32,
    pub data_bitmap_blocks: u32,
    pub data_area_blocks: u32,
}

pub struct BlockAllocator<'bitmap> {
    area_start: usize,
    bitmap: &'bitmap Bitmap,
}

impl<'bitmap> BlockAllocator<'bitmap> {
    pub fn new(area_start: usize, bitmap: &'bitmap Bitmap) -> Self {
        Self {
            area_start,
            bitmap,
        }
    }

    pub fn alloc<'a, 'b>(&'a self, cache_mgr: &'b mut BlockCacheManager) -> Option<&'b Arc<BlockCache>> {
        let slot = self.bitmap.alloc(cache_mgr)?;
        cache_mgr.get_block(self.area_start + slot).into()
    }

    pub fn dealloc(&self, block_id: usize, cache_mgr: &mut BlockCacheManager) {
        self.bitmap.dealloc(block_id.checked_sub(self.area_start).unwrap(), cache_mgr);
    }
}

// 32 * 4 = 128 bytes
#[derive(Debug, Clone)]
#[repr(C)]
pub struct DiskInode {
    pub size: u32,
    pub direct: [u32; INODE_DIRECT_COUNT],
    pub indirect: [u32; 2],
    ty: DiskInodeType,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskInodeType {
    File = 1,
    Directory = 2,
}

enum InnerId {
    Direct(usize),
    Indirect1(usize),
    Indirect2(usize, usize),
}

impl InnerId {
    fn new(inner_id: usize) -> Self {
        if inner_id < INODE_DIRECT_COUNT {
            Self::Direct(inner_id)
        } else if inner_id < INODE_DIRECT_COUNT + INODE_INDIRECT_COUNT {
            Self::Indirect1(inner_id - INODE_DIRECT_COUNT)
        } else if inner_id < INODE_DIRECT_COUNT + INODE_INDIRECT_COUNT + INODE_INDIRECT_COUNT.pow(2) {
            let idx = inner_id - INODE_DIRECT_COUNT - INODE_INDIRECT_COUNT;
            Self::Indirect2(idx / INODE_INDIRECT_COUNT, idx % INODE_INDIRECT_COUNT)
        } else {
            panic!("out-of-bound inner id")
        }
    }
}

impl DiskInode {
    pub fn new(ty: DiskInodeType) -> Self {
        Self {
            size: 0,
            direct: Default::default(),
            indirect: Default::default(),
            ty,
        }
    }

    pub fn file() -> Self {
        Self::new(DiskInodeType::File)
    }

    pub fn directory() -> Self {
        Self::new(DiskInodeType::Directory)
    }

    fn blocks_for_size(size: u32) -> usize {
        (size as usize).div_ceil(BLOCK_SIZE)
    }

    pub fn resize(&mut self, new_size: u32, block_allocator: &BlockAllocator, cache_mgr: &mut BlockCacheManager) {
        assert!(new_size as usize <= MAX_FILE_SIZE, "file size limit exceeded");

        let new_blocks = Self::blocks_for_size(new_size);
        let old_blocks = Self::blocks_for_size(self.size);
        if self.size < new_size {
            if self.size > 0 {
                // clear pass-the-end data at last block
                let last_block = cache_mgr.get_block(old_blocks - 1);
                let last_pos = self.size as usize % BLOCK_SIZE;
                let f = |b: &mut Block| b[last_pos..].fill(0);
                unsafe { last_block.modify(0, f) }
            }
            for inner_id in old_blocks..new_blocks {
                let new_block = block_allocator.alloc(cache_mgr).expect("cannot alloc more blocks");
                let f = |b: &mut Block| b.fill(0);
                unsafe { new_block.modify(0, f) };
                self.set_block_id(inner_id, new_block.block_id() as u32, block_allocator, cache_mgr);
            }
        } else if self.size > new_size {
            for inner_id in new_blocks..old_blocks {
                let deallocated = self.set_block_id(inner_id, 0, block_allocator, cache_mgr);
                block_allocator.dealloc(deallocated as usize, cache_mgr);
            }

            let new_last_block = InnerId::new(new_blocks - 1);
            let old_last_block = InnerId::new(old_blocks - 1);

            // dealloc unused indirect blocks
            use InnerId::*;
            match (new_last_block, old_last_block) {
                (Direct(_), Indirect1(_)) => {
                    block_allocator.dealloc(self.indirect[0] as usize, cache_mgr);
                    self.indirect[0] = 0;
                }
                (Indirect2(begin, _), Indirect2(end, _)) if begin < end => {
                    let indirect2 = Arc::clone(cache_mgr.get_block(self.indirect[1] as usize));
                    let f = |indirect2: &mut IndirectBlock| {
                        for i in (begin + 1)..=end {
                            block_allocator.dealloc(indirect2[i] as usize, cache_mgr);
                            indirect2[i] = 0;
                        }
                    };
                    unsafe { indirect2.modify(0, f) }
                }
                (Indirect1(_), Indirect2(indirect2_blocks, _)) => {
                    let indirect2 = Arc::clone(cache_mgr.get_block(self.indirect[1] as usize));
                    let f = |indirect2: &mut IndirectBlock| {
                        for i in 0..=indirect2_blocks {
                            block_allocator.dealloc(indirect2[i] as usize, cache_mgr);
                            indirect2[i] = 0;
                        }
                    };
                    unsafe { indirect2.modify(0, f) }
                    block_allocator.dealloc(self.indirect[1] as usize, cache_mgr);
                    self.indirect[1] = 0;
                }
                (Direct(_), Indirect2(indirect2_blocks, _)) => {
                    let indirect2 = Arc::clone(cache_mgr.get_block(self.indirect[1] as usize));
                    let f = |indirect2: &mut IndirectBlock| {
                        for i in 0..=indirect2_blocks {
                            block_allocator.dealloc(indirect2[i] as usize, cache_mgr);
                            indirect2[i] = 0;
                        }
                    };
                    unsafe { indirect2.modify(0, f) }
                    block_allocator.dealloc(self.indirect[0] as usize, cache_mgr);
                    self.indirect[0] = 0;
                    block_allocator.dealloc(self.indirect[1] as usize, cache_mgr);
                    self.indirect[1] = 0;
                }
                _ => (),
            }
        }
        self.size = new_size;
    }

    pub fn read_at(
        &self,
        offset: usize,
        buf: &mut [u8], 
        cache_mgr: &mut BlockCacheManager,
    ) {
        let start_inner_id = offset / BLOCK_SIZE;
        let start_offset = offset % BLOCK_SIZE;

        let mut inner_id = start_inner_id;
        let mut block_start = start_offset;
        let mut buf_start = 0;
        while buf_start < buf.len() {
            let block_id = self.get_block_id(inner_id, cache_mgr);
            let block = cache_mgr.get_block(block_id as usize);
            let f = |b: &Block| {
                let block_end = core::cmp::min(block_start + buf[buf_start..].len(), BLOCK_SIZE);
                let buf_end = buf_start + block_end - block_start;
                buf[buf_start..buf_end].copy_from_slice(&b[block_start..block_end]);
                buf_start = buf_end;
            };
            unsafe { block.read(0, f) }
            inner_id += 1;
            block_start = 0;
        }
    }

    pub fn write_at(
        &mut self,
        offset: usize,
        data: &[u8], 
        block_allocator: &BlockAllocator,
        cache_mgr: &mut BlockCacheManager,
    ) {
        self.resize((offset + data.len()) as u32, block_allocator, cache_mgr);
        let start_inner_id = offset / BLOCK_SIZE;
        let start_offset = offset % BLOCK_SIZE;

        let mut inner_id = start_inner_id;
        let mut block_start = start_offset;
        let mut data_start = 0;
        while data_start < data.len() {
            let block_id = self.get_block_id(inner_id, cache_mgr);
            let block = cache_mgr.get_block(block_id as usize);
            let f = |b: &mut Block| {
                let data_end = data_start + core::cmp::min(data[data_start..].len(), BLOCK_SIZE - block_start);
                let block_end = block_start + data_end - data_start;
                b[block_start..block_end].copy_from_slice(&data[data_start..data_end]);
                data_start = data_end
            };
            unsafe { block.modify(0, f) }
            inner_id += 1;
            block_start = 0;
        }
    }

    pub fn get_block_id(&self, inner_id: usize, cache_mgr: &mut BlockCacheManager) -> u32 {
        match InnerId::new(inner_id) {
            InnerId::Direct(id) => {
                self.direct[id]
            }
            InnerId::Indirect1(id) => {
                let indirect1 = cache_mgr.get_block(self.indirect[0] as usize);
                let f = |indirect1: &IndirectBlock| indirect1[id];
                // SAFETY: arbitrary initialized data would be valid for this type
                unsafe { indirect1.read(0, f) }
            }
            InnerId::Indirect2(id1, id2) => {
                let indirect2 = {
                    let indirect1 = cache_mgr.get_block(self.indirect[0] as usize);
                    let f = |indirect1: &IndirectBlock| indirect1[id1];
                    let indirect2_block_id = unsafe { indirect1.read(0, f) };
                    cache_mgr.get_block(indirect2_block_id as usize)
                };
                let f = |indirect2: &IndirectBlock| indirect2[id2];
                unsafe { indirect2.read(0, f) }
            }
        }
    }

    pub fn set_block_id(&mut self, inner_id: usize, block_id: u32, block_allocator: &BlockAllocator, cache_mgr: &mut BlockCacheManager) -> u32 {
        match InnerId::new(inner_id) {
            InnerId::Direct(id) => {
                mem::replace(&mut self.direct[id], block_id)
            }
            InnerId::Indirect1(id) => {
                let indirect1 = if self.indirect[0] != 0 {
                    cache_mgr.get_block(self.indirect[0] as usize)
                } else {
                    block_allocator.alloc(cache_mgr).expect("we run out of blocks. QAQ")
                };
                let f = |indirect1: &mut IndirectBlock| mem::replace(&mut indirect1[id], block_id);
                // SAFETY: arbitrary initialized data would be valid for this type
                unsafe { indirect1.modify(0, f) }
            }
            InnerId::Indirect2(id1, id2) => {
                let indirect2 = {
                    let indirect1 = cache_mgr.get_block(self.indirect[0] as usize);
                    let f = |indirect1: &IndirectBlock| indirect1[id1];
                    let indirect2_block_id = unsafe { indirect1.read(0, f) };
                    cache_mgr.get_block(indirect2_block_id as usize)
                };
                let f = |indirect2: &mut IndirectBlock| mem::replace(&mut indirect2[id2], block_id);
                unsafe { indirect2.modify(0, f) }
            }
        }
    }

}

pub struct Inode {
    block_id: usize,
    offset: usize,
}

impl Inode {
    #[track_caller]
    pub fn new(block_id: usize, offset: usize) -> Self {
        assert!(offset % core::mem::size_of::<DiskInode>() == 0, "offset doesn't align");

        Self {
            block_id,
            offset,
        }
    }

    pub fn create(block_id: usize, offset: usize, ty: DiskInodeType, cache_mgr: &mut BlockCacheManager) -> Self {
        assert!(offset % core::mem::size_of::<DiskInode>() == 0, "offset doesn't align");

        let block = cache_mgr.get_block(block_id);
        let f = |di: &mut MaybeUninit<DiskInode>| {
            di.write(DiskInode::new(ty));
        };
        block.modify_maybe_uninit(offset, f);
        Self {
            block_id,
            offset,
        }
    }

    pub fn create_file(block_id: usize, offset: usize, cache_mgr: &mut BlockCacheManager) -> Self {
        Self::create(block_id, offset, DiskInodeType::File, cache_mgr)
    }

    pub fn create_dir(block_id: usize, offset: usize, cache_mgr: &mut BlockCacheManager) -> Self {
        Self::create(block_id, offset, DiskInodeType::Directory, cache_mgr)
    }

    pub unsafe fn resize(&self, new_size: u32, block_allocator: &BlockAllocator, cache_mgr: &mut BlockCacheManager) {
        let block = Arc::clone(cache_mgr.get_block(self.block_id));
        let f = |di: &mut DiskInode| {
            di.resize(new_size, block_allocator, cache_mgr);
        };
        block.modify(self.offset, f);
    }

    pub unsafe fn read_at(&self, offset: usize, buf: &mut [u8], cache_mgr: &mut BlockCacheManager) {
        let block = Arc::clone(cache_mgr.get_block(self.block_id));
        let f = |di: &DiskInode| {
            di.read_at(offset, buf, cache_mgr)
        };
        block.read(self.offset, f);
    }

    pub unsafe fn write_at(&self, offset: usize, data: &[u8], block_allocator: &BlockAllocator, cache_mgr: &mut BlockCacheManager) {
        let block = Arc::clone(cache_mgr.get_block(self.block_id));
        let f = |di: &mut DiskInode| {
            di.write_at(offset, data, block_allocator, cache_mgr)
        };
        block.modify(self.offset, f);
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_cache::tests::setup;

    #[test]
    fn inode_basic() {
        let (_block_dev, mut cache_mgr) = setup();
        let bitmap = Bitmap::new(0, 4);
        let block_allocator = BlockAllocator::new(10, &bitmap);

        let inode_block = block_allocator.alloc(&mut cache_mgr).unwrap();
        let inode = Inode::create_file(inode_block.block_id(), 0, &mut cache_mgr);

        let data = b"hello, world!";
        let mut buf = [0; 13];
        unsafe {
            inode.write_at(510, data, &block_allocator, &mut cache_mgr);
            inode.read_at(510, &mut buf, &mut cache_mgr);
        }
        assert_eq!(data, &buf);
    }
}
