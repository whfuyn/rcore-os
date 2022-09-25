use alloc::sync::Arc;

use core::mem;
use core::cmp;
use crate::{BLOCK_SIZE, Block};
use crate::block_cache::BlockCacheManager;
use crate::block_cache::BlockCache;
use crate::bitmap::Bitmap;
use core::mem::MaybeUninit;
use crate::efs::EasyFileSystem;

pub const EASY_FS_MAGIC: u32 = 0xf1f1f1f1;
const INODE_DIRECT_COUNT: usize = 28;
const INODE_INDIRECT_COUNT: usize = BLOCK_SIZE / core::mem::size_of::<u32>();

const MAX_FILE_SIZE: usize = (INODE_DIRECT_COUNT + INODE_INDIRECT_COUNT + INODE_INDIRECT_COUNT.pow(2)) * BLOCK_SIZE;
const MAX_NAME_LENGTH: usize = 27;

type IndirectBlock = [u32; INODE_INDIRECT_COUNT];

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SuperBlock {
    magic: u32,
    pub total_blocks: u32,
    pub inode_bitmap_blocks: u32,
    pub inode_area_blocks: u32,
    pub data_bitmap_blocks: u32,
    pub data_area_blocks: u32,
}

impl SuperBlock {
    pub fn new(
        total_blocks: u32,
        inode_bitmap_blocks: u32,
        inode_area_blocks: u32,
        data_bitmap_blocks: u32,
        data_area_blocks: u32,
    ) -> Self {
        let it = Self {
            magic: EASY_FS_MAGIC,
            total_blocks,
            inode_bitmap_blocks,
            inode_area_blocks,
            data_bitmap_blocks,
            data_area_blocks,
        };
        assert!(it.validate(), "insufficient total blocks");
        it
    }

    pub fn validate(&self) -> bool {
        let sum = self.inode_bitmap_blocks + self.inode_area_blocks + self.data_bitmap_blocks + self.data_area_blocks + 1;
        self.total_blocks < sum && self.magic == EASY_FS_MAGIC
    }

}

// 32 * 4 = 128 bytes
#[derive(Debug, Clone)]
#[repr(C)]
pub struct DiskInode {
    pub size: u32,
    pub direct: [u32; INODE_DIRECT_COUNT],
    pub indirect: [u32; 2],
    pub ty: InodeType,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InodeType {
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
    pub fn new(ty: InodeType) -> Self {
        Self {
            size: 0,
            direct: Default::default(),
            indirect: Default::default(),
            ty,
        }
    }

    pub fn file() -> Self {
        Self::new(InodeType::File)
    }

    pub fn directory() -> Self {
        Self::new(InodeType::Directory)
    }

    fn blocks_for_size(size: u32) -> usize {
        (size as usize).div_ceil(BLOCK_SIZE)
    }

    pub fn resize(&mut self, new_size: u32, fs: &EasyFileSystem) {
        assert!(new_size as usize <= MAX_FILE_SIZE, "file size limit exceeded");

        let new_blocks = Self::blocks_for_size(new_size);
        let old_blocks = Self::blocks_for_size(self.size);
        if self.size < new_size {
            if self.size > 0 {
                // clear pass-the-end data at last block
                let last_block = fs.get_block(old_blocks - 1);
                let last_pos = self.size as usize % BLOCK_SIZE;
                let f = |b: &mut Block| b[last_pos..].fill(0);
                unsafe { last_block.modify(0, f) }
            }
            for inner_id in old_blocks..new_blocks {
                let new_block = fs.alloc_block().expect("cannot alloc more blocks");
                let f = |b: &mut Block| b.fill(0);
                unsafe { new_block.modify(0, f) };
                self.set_block_id(inner_id, new_block.block_id() as u32, fs);
            }
        } else if self.size > new_size {
            for inner_id in new_blocks..old_blocks {
                let deallocated = self.set_block_id(inner_id, 0, fs);
                fs.dealloc_block(deallocated as usize);
            }

            let new_last_block = InnerId::new(new_blocks - 1);
            let old_last_block = InnerId::new(old_blocks - 1);

            // dealloc unused indirect blocks
            use InnerId::*;
            match (new_last_block, old_last_block) {
                (Direct(_), Indirect1(_)) => {
                    fs.dealloc_block(self.indirect[0] as usize);
                    self.indirect[0] = 0;
                }
                (Indirect2(begin, _), Indirect2(end, _)) if begin < end => {
                    let indirect2 = fs.get_block(self.indirect[1] as usize);
                    let f = |indirect2: &mut IndirectBlock| {
                        for i in (begin + 1)..=end {
                            fs.dealloc_block(indirect2[i] as usize);
                            indirect2[i] = 0;
                        }
                    };
                    unsafe { indirect2.modify(0, f) }
                }
                (Indirect1(_), Indirect2(indirect2_blocks, _)) => {
                    // let indirect2 = Arc::clone(cache_mgr.get_block(self.indirect[1] as usize));
                    let indirect2 = fs.get_block(self.indirect[1] as usize);
                    let f = |indirect2: &mut IndirectBlock| {
                        for i in 0..=indirect2_blocks {
                            fs.dealloc_block(indirect2[i] as usize);
                            indirect2[i] = 0;
                        }
                    };
                    unsafe { indirect2.modify(0, f) }
                    fs.dealloc_block(self.indirect[1] as usize);
                    self.indirect[1] = 0;
                }
                (Direct(_), Indirect2(indirect2_blocks, _)) => {
                    let indirect2 = fs.get_block(self.indirect[1] as usize);
                    let f = |indirect2: &mut IndirectBlock| {
                        for i in 0..=indirect2_blocks {
                            fs.dealloc_block(indirect2[i] as usize);
                            indirect2[i] = 0;
                        }
                    };
                    unsafe { indirect2.modify(0, f) }
                    fs.dealloc_block(self.indirect[0] as usize);
                    self.indirect[0] = 0;
                    fs.dealloc_block(self.indirect[1] as usize);
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
        fs: &EasyFileSystem,
    ) -> usize {
        let start_inner_id = offset / BLOCK_SIZE;
        let start_offset = offset % BLOCK_SIZE;

        let mut inner_id = start_inner_id;
        let mut block_start = start_offset;
        let mut buf_start = 0;
        let mut remain = (self.size as usize).saturating_sub(offset);
        while buf_start < buf.len() && remain > 0 {
            let block_id = self.get_block_id(inner_id, fs);
            let block = fs.get_block(block_id as usize);
            let f = |b: &Block| {
                let n = {
                    let v = cmp::min(buf[buf_start..].len(), BLOCK_SIZE - block_start);
                    cmp::min(v, remain)
                };
                let block_end = block_start + n;
                let buf_end = buf_start + n;
                buf[buf_start..buf_end].copy_from_slice(&b[block_start..block_end]);
                buf_start = buf_end;
                remain -= n;
            };
            unsafe { block.read(0, f) }
            inner_id += 1;
            block_start = 0;
        }
        // bytes readed
        buf_start
    }

    pub fn write_at(
        &mut self,
        offset: usize,
        data: &[u8], 
        fs: &EasyFileSystem,
    ) {
        self.resize((offset + data.len()) as u32, fs);
        let start_inner_id = offset / BLOCK_SIZE;
        let start_offset = offset % BLOCK_SIZE;

        let mut inner_id = start_inner_id;
        let mut block_start = start_offset;
        let mut data_start = 0;
        while data_start < data.len() {
            let block_id = self.get_block_id(inner_id, fs);
            let block = fs.get_block(block_id as usize);
            let f = |b: &mut Block| {
                let data_end = data_start + cmp::min(data[data_start..].len(), BLOCK_SIZE - block_start);
                let block_end = block_start + data_end - data_start;
                b[block_start..block_end].copy_from_slice(&data[data_start..data_end]);
                data_start = data_end
            };
            unsafe { block.modify(0, f) }
            inner_id += 1;
            block_start = 0;
        }
    }

    pub fn get_block_id(&self, inner_id: usize, fs: &EasyFileSystem) -> u32 {
        match InnerId::new(inner_id) {
            InnerId::Direct(id) => {
                self.direct[id]
            }
            InnerId::Indirect1(id) => {
                let indirect1 = fs.get_block(self.indirect[0] as usize);
                let f = |indirect1: &IndirectBlock| indirect1[id];
                // SAFETY: arbitrary initialized data would be valid for this type
                unsafe { indirect1.read(0, f) }
            }
            InnerId::Indirect2(id1, id2) => {
                let indirect2 = {
                    let indirect1 = fs.get_block(self.indirect[0] as usize);
                    let f = |indirect1: &IndirectBlock| indirect1[id1];
                    let indirect2_block_id = unsafe { indirect1.read(0, f) };
                    fs.get_block(indirect2_block_id as usize)
                };
                let f = |indirect2: &IndirectBlock| indirect2[id2];
                unsafe { indirect2.read(0, f) }
            }
        }
    }

    pub fn set_block_id(&mut self, inner_id: usize, block_id: u32, fs: &EasyFileSystem) -> u32 {
        match InnerId::new(inner_id) {
            InnerId::Direct(id) => {
                mem::replace(&mut self.direct[id], block_id)
            }
            InnerId::Indirect1(id) => {
                let indirect1 = if self.indirect[0] != 0 {
                    fs.get_block(self.indirect[0] as usize)
                } else {
                    fs.alloc_block().expect("we run out of blocks. QAQ")
                };
                let f = |indirect1: &mut IndirectBlock| mem::replace(&mut indirect1[id], block_id);
                // SAFETY: arbitrary initialized data would be valid for this type
                unsafe { indirect1.modify(0, f) }
            }
            InnerId::Indirect2(id1, id2) => {
                let indirect2 = {
                    let indirect1 = fs.get_block(self.indirect[0] as usize);
                    let f = |indirect1: &IndirectBlock| indirect1[id1];
                    let indirect2_block_id = unsafe { indirect1.read(0, f) };
                    fs.get_block(indirect2_block_id as usize)
                };
                let f = |indirect2: &mut IndirectBlock| mem::replace(&mut indirect2[id2], block_id);
                unsafe { indirect2.modify(0, f) }
            }
        }
    }
}

#[repr(C)]
pub struct DirEntry {
    name: [u8; MAX_NAME_LENGTH + 1],
    inode_id: u32,
}

