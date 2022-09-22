use alloc::sync::Arc;

use core::mem;
use crate::{BLOCK_SIZE, Block};
use crate::block_cache::BlockCacheManager;
use crate::bitmap::Bitmap;

const EASY_FS_MAGIC: u32 = 0x666;
const INODE_DIRECT_COUNT: usize = 28;
const INODE_INDIRECT_COUNT: usize = BLOCK_SIZE / core::mem::size_of::<u32>();

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

#[repr(C)]
pub struct DiskInode {
    pub size: u32,
    pub direct: [u32; INODE_DIRECT_COUNT],
    pub indirect: [u32; 2],
    ty: DiskInodeType,
}

#[repr(u32)]
#[derive(Debug, PartialEq, Eq)]
pub enum DiskInodeType {
    File,
    Directory,
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

    fn resize(&mut self, size: u32, bitmap: &Bitmap, cache_mgr: &mut BlockCacheManager) {
        let new_blocks = Self::blocks_for_size(size);
        let old_blocks = Self::blocks_for_size(self.size);
        if self.size < size {
            if self.size > 0 {
                // clear pass-the-end data at last block
                let last_block = cache_mgr.get_block(old_blocks - 1);
                let last_pos = self.size as usize % BLOCK_SIZE;
                let f = |b: &mut Block| b[last_pos..].fill(0);
                unsafe { last_block.modify(0, f) }
            }
            for inner_id in old_blocks..new_blocks {
                let new_block_id = bitmap.alloc(cache_mgr).expect("cannot alloc more blocks");
                let block = cache_mgr.get_block(new_block_id);
                let f = |b: &mut Block| b.fill(0);
                unsafe { block.modify(0, f) };
                self.set_block_id(inner_id, new_block_id as u32, bitmap, cache_mgr);
            }
        } else if self.size > size {
            for inner_id in new_blocks..old_blocks {
                let deallocated = self.set_block_id(inner_id, 0, bitmap, cache_mgr);
                bitmap.dealloc(deallocated as usize, cache_mgr);
            }

            let new_last_block = InnerId::new(new_blocks - 1);
            let old_last_block = InnerId::new(old_blocks - 1);

            // dealloc unused indirect blocks
            use InnerId::*;
            match (new_last_block, old_last_block) {
                (Direct(_), Indirect1(_)) => {
                    bitmap.dealloc(self.indirect[0] as usize, cache_mgr);
                    self.indirect[0] = 0;
                }
                (Indirect2(begin, _), Indirect2(end, _)) if begin < end => {
                    let indirect2 = Arc::clone(cache_mgr.get_block(self.indirect[1] as usize));
                    let f = |indirect2: &mut IndirectBlock| {
                        for i in (begin + 1)..=end {
                            bitmap.dealloc(indirect2[i] as usize, cache_mgr);
                            indirect2[i] = 0;
                        }
                    };
                    unsafe { indirect2.modify(0, f) }
                }
                (Indirect1(_), Indirect2(indirect2_blocks, _)) => {
                    let indirect2 = Arc::clone(cache_mgr.get_block(self.indirect[1] as usize));
                    let f = |indirect2: &mut IndirectBlock| {
                        for i in 0..=indirect2_blocks {
                            bitmap.dealloc(indirect2[i] as usize, cache_mgr);
                            indirect2[i] = 0;
                        }
                    };
                    unsafe { indirect2.modify(0, f) }
                    bitmap.dealloc(self.indirect[1] as usize, cache_mgr);
                    self.indirect[1] = 0;
                }
                (Direct(_), Indirect2(indirect2_blocks, _)) => {
                    let indirect2 = Arc::clone(cache_mgr.get_block(self.indirect[1] as usize));
                    let f = |indirect2: &mut IndirectBlock| {
                        for i in 0..=indirect2_blocks {
                            bitmap.dealloc(indirect2[i] as usize, cache_mgr);
                            indirect2[i] = 0;
                        }
                    };
                    unsafe { indirect2.modify(0, f) }
                    bitmap.dealloc(self.indirect[0] as usize, cache_mgr);
                    self.indirect[0] = 0;
                    bitmap.dealloc(self.indirect[1] as usize, cache_mgr);
                    self.indirect[1] = 0;
                }
                _ => (),
            }
        }
    }

    pub fn write_at(
        &self,
        offset: usize,
        data: &[u8], 
        bitmap: &Bitmap,
        cache_mgr: &mut BlockCacheManager,
    ) {
        todo!()
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

    pub fn set_block_id(&mut self, inner_id: usize, block_id: u32, bitmap: &Bitmap, cache_mgr: &mut BlockCacheManager) -> u32 {
        match InnerId::new(inner_id) {
            InnerId::Direct(id) => {
                mem::replace(&mut self.direct[id], block_id)
            }
            InnerId::Indirect1(id) => {
                let indirect1_block_id = if self.indirect[0] != 0 {
                    self.indirect[0] as usize
                } else {
                    bitmap.alloc(cache_mgr).expect("we run out of blocks. QAQ")
                };
                let indirect1 = cache_mgr.get_block(indirect1_block_id);
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
