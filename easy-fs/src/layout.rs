use crate::efs::EasyFileSystem;
use crate::{Block, BLOCK_SIZE, BLOCK_BITS};
use bitflags::bitflags;
use core::cmp;
use static_assertions::assert_eq_size;

pub const EASY_FS_MAGIC: u32 = 0xf1f1f1f1;
const INODE_DIRECT_COUNT: usize = 28;
const INODE_INDIRECT_COUNT: usize = BLOCK_SIZE / core::mem::size_of::<u32>();

const MAX_FILE_SIZE: usize =
    (INODE_DIRECT_COUNT + INODE_INDIRECT_COUNT + INODE_INDIRECT_COUNT.pow(2)) * BLOCK_SIZE;
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
        self.magic == EASY_FS_MAGIC
            && self.inode_bitmap_blocks * BLOCK_BITS as u32 <= self.inode_area_blocks
            && self.data_bitmap_blocks * BLOCK_BITS as u32 <= self.data_area_blocks
            && 1 + self.inode_bitmap_blocks + self.inode_area_blocks 
                + self.data_bitmap_blocks + self.data_area_blocks <= self.total_blocks
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

assert_eq_size!(DiskInode, [u8; 128]);

// Don't use rust enum for this type.
// Data on disk might be corrupted, and transmuting a invalid value to
// rust enum is UB.
bitflags! {
    #[repr(transparent)]
    pub struct InodeType: u32 {
        const FILE = 1;
        const DIRECTORY = 2;
    }
}

impl InodeType {
    pub fn validate(self) {
        assert!(
            self == Self::FILE || self == Self::DIRECTORY,
            "invalid inode type. Data might be corrupted",
        );
    }

    pub fn is_file(self) -> bool {
        self.validate();
        self == Self::FILE
    }

    pub fn is_dir(self) -> bool {
        self.validate();
        self == Self::DIRECTORY
    }
}

#[derive(Debug)]
enum InnerIndex {
    Direct(usize),
    Indirect1(usize),
    Indirect2(usize, usize),
}

impl InnerIndex {
    fn new(inner_id: usize) -> Self {
        if inner_id < INODE_DIRECT_COUNT {
            Self::Direct(inner_id)
        } else if inner_id < INODE_DIRECT_COUNT + INODE_INDIRECT_COUNT {
            Self::Indirect1(inner_id - INODE_DIRECT_COUNT)
        } else if inner_id
            < INODE_DIRECT_COUNT + INODE_INDIRECT_COUNT + INODE_INDIRECT_COUNT.pow(2)
        {
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

    fn blocks_for_size(size: u32) -> usize {
        (size as usize).div_ceil(BLOCK_SIZE)
    }

    fn increase_size(&mut self, new_size: u32, fs: &EasyFileSystem) {
        assert!(
            new_size as usize <= MAX_FILE_SIZE,
            "file size limit exceeded"
        );
        let old_blocks = Self::blocks_for_size(self.size);
        let new_blocks = Self::blocks_for_size(new_size);

        if old_blocks >= new_blocks {
            return;
        }

        let alloc_zeroed_block = || {
            let new_block = fs.alloc_block().expect("cannot alloc more blocks");
            unsafe {
                new_block.modify(0, |b: &mut Block| b.fill(0));
            }
            new_block
        };

        if self.size as usize % BLOCK_SIZE > 0 {
            // clear pass-the-end data at old last block
            let last_block_id = self.get_block_id(old_blocks - 1, fs);
            let last_block = fs.get_block(last_block_id as usize);
            let last_pos = self.size as usize % BLOCK_SIZE;
            let f = |b: &mut Block| b[last_pos..].fill(0);
            unsafe { last_block.modify(0, f) }
        }

        let mut k = old_blocks;
        'out: while k < new_blocks {
            while let InnerIndex::Direct(i) = InnerIndex::new(k) {
                self.direct[i] = alloc_zeroed_block().block_id() as u32;

                k += 1;
                if k >= new_blocks {
                    break 'out;
                }
            }

            let indirect1 = if self.indirect[0] == 0 {
                let indirect1 = alloc_zeroed_block();
                self.indirect[0] = indirect1.block_id() as u32;
                indirect1
            } else {
                fs.get_block(self.indirect[0] as usize)
            };
            let f = |indirect1: &mut IndirectBlock| {
                while let InnerIndex::Indirect1(i) = InnerIndex::new(k) {
                    indirect1[i] = alloc_zeroed_block().block_id() as u32;

                    k += 1;
                    if k >= new_blocks {
                        return false;
                    }
                }
                true
            };
            let has_more =  unsafe { indirect1.modify(0, f) };
            if !has_more {
                break;
            }

            let indirect2_1 = if self.indirect[1] == 0 {
                let indirect2_1 = alloc_zeroed_block();
                self.indirect[1] = indirect2_1.block_id() as u32;
                indirect2_1
            } else {
                fs.get_block(self.indirect[1] as usize)
            };
            let f = |indirect2_1: &mut IndirectBlock| {
                let InnerIndex::Indirect2(mut indirect2_2_i, mut indirect2_2_j) = InnerIndex::new(k) else { unreachable!() };
                loop {
                    let indirect2_2 = if indirect2_1[indirect2_2_i] == 0 {
                        let indirect2_2 = alloc_zeroed_block();
                        indirect2_1[indirect2_2_i] = indirect2_2.block_id() as u32;
                        indirect2_2
                    } else {
                        fs.get_block(indirect2_1[indirect2_2_i] as usize)
                    };
                    while indirect2_2_j < INODE_INDIRECT_COUNT {
                        let f = |indirect2_2: &mut IndirectBlock| {
                            indirect2_2[indirect2_2_j] = alloc_zeroed_block().block_id() as u32;
                        };
                        unsafe { indirect2_2.modify(0, f) }

                        indirect2_2_j += 1;
                        k += 1;
                        if k >= new_blocks {
                            return;
                        }
                    }
                    indirect2_2_i += 1;
                    indirect2_2_j = 0;
                }
            };
            unsafe { indirect2_1.modify(0, f) }
        }
    }

    fn decrease_size(&mut self, new_size: u32, fs: &EasyFileSystem) {
        if self.size == 0 {
            return;
        }

        let old_blocks = Self::blocks_for_size(self.size);
        let new_blocks = Self::blocks_for_size(new_size);
        if old_blocks <= new_blocks {
            return;
        }
        // [to <- from]
        let to = InnerIndex::new(new_blocks);
        let mut from = InnerIndex::new(old_blocks - 1);

        if let InnerIndex::Indirect2(mut from_i, mut from_j) = from {
            let indirect2_1 = fs.get_block(self.indirect[1] as usize);
            let (to_i, to_j) = match to {
                InnerIndex::Indirect2(i, j) => (i, j),
                _ => (0, 0),
            };
            let f = |indirect2_1: &mut IndirectBlock| {
                for i in (to_i + 1..=from_i).rev() {
                    let indirect2_2 = fs.get_block(indirect2_1[i] as usize);
                    let g = |indirect2_2: &mut IndirectBlock| {
                        for j in (0..=from_j).rev() {
                            fs.dealloc_block(indirect2_2[j] as usize);
                            indirect2_2[j] = 0;
                        }
                    };
                    unsafe { indirect2_2.modify(0, g)} 
                    fs.dealloc_block(indirect2_2.block_id());
                    indirect2_1[i] = 0;
                    from_j = INODE_INDIRECT_COUNT - 1;
                }

                let indirect2_2 = fs.get_block(indirect2_1[to_i] as usize);
                let g = |indirect2_2: &mut IndirectBlock| {
                    for j in (to_j..=from_j).rev() {
                        fs.dealloc_block(indirect2_2[j] as usize);
                        indirect2_2[j] = 0;
                    }
                };
                unsafe { indirect2_2.modify(0, g) }
                if to_j == 0 {
                    fs.dealloc_block(indirect2_1[to_i] as usize);
                    indirect2_1[to_i] = 0;
                }
                from_i = to_i;
                from_j = to_j;
            };
            unsafe { indirect2_1.modify(0, f)} 
            if from_i == 0 && from_j == 0 {
                fs.dealloc_block(self.indirect[1] as usize);
                self.indirect[1] = 0;
                if !matches!(to, InnerIndex::Indirect2(0, 0)) {
                    from = InnerIndex::Indirect1(INODE_INDIRECT_COUNT - 1);
                }
            }
        }

        if let InnerIndex::Indirect1(mut from_i) = from {
            let to_i = match to {
                InnerIndex::Indirect1(i) => i,
                _ => 0,
            };
            let indirect1 = fs.get_block(self.indirect[0] as usize);
            let f = |indirect1: &mut IndirectBlock| {
                for i in (to_i..=from_i).rev() {
                    fs.dealloc_block(indirect1[i] as usize);
                    indirect1[i] = 0;
                }
                from_i = to_i;
            };
            unsafe { indirect1.modify(0, f) }
            if from_i == 0 {
                fs.dealloc_block(self.indirect[0] as usize);
                self.indirect[0] = 0;
                if !matches!(to, InnerIndex::Indirect1(0)) {
                    from = InnerIndex::Direct(INODE_DIRECT_COUNT);
                }
            }
        }

        if let InnerIndex::Direct(from_i) = from {
            let to_i = match to {
                InnerIndex::Direct(i) => i,
                _ => unreachable!(),
            };
            for i in (to_i..=from_i).rev() {
                fs.dealloc_block(self.direct[i] as usize);
                self.direct[i] = 0;
            }
        }
    }

    pub fn resize(&mut self, new_size: u32, fs: &EasyFileSystem) {
        if self.size < new_size {
            self.increase_size(new_size, fs);
        } else if self.size > new_size {
            self.decrease_size(new_size, fs);
        }
        self.size = new_size;
    }

    // fn alloc_fresh_block(fs: &EasyFileSystem) -> Arc<BlockCache> {
    //     let new_block = fs.alloc_block().expect("cannot alloc more blocks");
    //     unsafe {
    //         new_block.modify(0, |b: &mut Block| b.fill(0));
    //     }
    //     new_block
    // }

    // pub fn resize(&mut self, new_size: u32, fs: &EasyFileSystem) {
    //     assert!(
    //         new_size as usize <= MAX_FILE_SIZE,
    //         "file size limit exceeded"
    //     );

    //     let old_blocks = Self::blocks_for_size(self.size);
    //     let new_blocks = Self::blocks_for_size(new_size);
    //     if self.size < new_size {
    //         if self.size as usize % BLOCK_SIZE > 0 {
    //             // clear pass-the-end data at last block
    //             let last_block_id = self.get_block_id(old_blocks - 1, fs);
    //             let last_block = fs.get_block(last_block_id as usize);
    //             let last_pos = self.size as usize % BLOCK_SIZE;
    //             let f = |b: &mut Block| b[last_pos..].fill(0);
    //             unsafe { last_block.modify(0, f) }
    //         }
    //         for inner_id in old_blocks..new_blocks {
    //             let new_block = fs.alloc_block().expect("cannot alloc more blocks");
    //             let f = |b: &mut Block| b.fill(0);
    //             unsafe { new_block.modify(0, f) };
    //             self.set_block_id(inner_id, new_block.block_id() as u32, fs);
    //         }
    //     } else if self.size > new_size {
    //         for inner_id in new_blocks..old_blocks {
    //             let deallocated = self.set_block_id(inner_id, 0, fs);
    //             fs.dealloc_block(deallocated as usize);
    //         }

    //         let new_last_block = InnerIndex::new(new_blocks);
    //         let old_last_block = InnerIndex::new(old_blocks);

    //         // dealloc unused indirect blocks
    //         use InnerIndex::*;
    //         match (new_last_block, old_last_block) {
    //             (Direct(_), Indirect1(_)) => {
    //                 fs.dealloc_block(self.indirect[0] as usize);
    //                 self.indirect[0] = 0;
    //             }
    //             (Indirect2(begin, _), Indirect2(end, _)) if begin < end => {
    //                 let indirect2 = fs.get_block(self.indirect[1] as usize);
    //                 let f = |indirect2: &mut IndirectBlock| {
    //                     for i in begin..end {
    //                         fs.dealloc_block(indirect2[i] as usize);
    //                         indirect2[i] = 0;
    //                     }
    //                 };
    //                 unsafe { indirect2.modify(0, f) }
    //             }
    //             (Indirect1(_), Indirect2(indirect2_blocks, _)) => {
    //                 let indirect2 = fs.get_block(self.indirect[1] as usize);
    //                 let f = |indirect2: &mut IndirectBlock| {
    //                     for i in 0..indirect2_blocks {
    //                         fs.dealloc_block(indirect2[i] as usize);
    //                         indirect2[i] = 0;
    //                     }
    //                 };
    //                 unsafe { indirect2.modify(0, f) }
    //                 fs.dealloc_block(self.indirect[1] as usize);
    //                 self.indirect[1] = 0;
    //             }
    //             (Direct(_), Indirect2(indirect2_blocks, _)) => {
    //                 let indirect2 = fs.get_block(self.indirect[1] as usize);
    //                 let f = |indirect2: &mut IndirectBlock| {
    //                     for i in 0..indirect2_blocks {
    //                         fs.dealloc_block(indirect2[i] as usize);
    //                         indirect2[i] = 0;
    //                     }
    //                 };
    //                 unsafe { indirect2.modify(0, f) }
    //                 fs.dealloc_block(self.indirect[0] as usize);
    //                 self.indirect[0] = 0;
    //                 fs.dealloc_block(self.indirect[1] as usize);
    //                 self.indirect[1] = 0;
    //             }
    //             _ => (),
    //         }
    //     }
    //     self.size = new_size;
    // }

    pub fn read_at(&self, offset: usize, buf: &mut [u8], fs: &EasyFileSystem) -> usize {
        let (start_inner_id, start_offset) = Self::offset_to_inner(offset);

        let mut inner_id = start_inner_id;
        let mut block_start = start_offset;
        let mut buf_start = 0;
        let mut remain = (self.size as usize).saturating_sub(offset);
        while buf_start < buf.len() && remain > 0 {
            let block = {
                let block_id = self.get_block_id(inner_id, fs);
                assert!(block_id != 0);
                fs.get_block(block_id as usize)
            };
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

    pub fn write_at(&mut self, offset: usize, data: &[u8], fs: &EasyFileSystem) {
        if offset + data.len() > self.size as usize {
            self.resize((offset + data.len()) as u32, fs);
        }
        let (start_inner_id, start_offset) = Self::offset_to_inner(offset);

        let mut inner_id = start_inner_id;
        let mut block_start = start_offset;
        let mut data_start = 0;
        while data_start < data.len() {
            let block = {
                let block_id = self.get_block_id(inner_id, fs);
                assert!(block_id != 0);
                fs.get_block(block_id as usize)
            };
            let f = |b: &mut Block| {
                let data_end =
                    data_start + cmp::min(data[data_start..].len(), BLOCK_SIZE - block_start);
                let block_end = block_start + data_end - data_start;
                b[block_start..block_end].copy_from_slice(&data[data_start..data_end]);
                data_start = data_end
            };
            unsafe { block.modify(0, f) }
            inner_id += 1;
            block_start = 0;
        }
    }

    fn get_block_id(&self, inner_id: usize, fs: &EasyFileSystem) -> u32 {
        match InnerIndex::new(inner_id) {
            InnerIndex::Direct(id) => self.direct[id],
            InnerIndex::Indirect1(id) => {
                let indirect1 = fs.get_block(self.indirect[0] as usize);
                let f = |indirect1: &IndirectBlock| indirect1[id];
                // SAFETY: arbitrary initialized data would be valid for this type
                unsafe { indirect1.read(0, f) }
            }
            InnerIndex::Indirect2(id1, id2) => {
                let indirect2_2 = {
                    let indirect2_1 = fs.get_block(self.indirect[1] as usize);
                    let f = |indirect2_1: &IndirectBlock| indirect2_1[id1];
                    let indirect2_2_block_id = unsafe { indirect2_1.read(0, f) };
                    fs.get_block(indirect2_2_block_id as usize)
                };
                let f = |indirect2_2: &IndirectBlock| indirect2_2[id2];
                unsafe { indirect2_2.read(0, f) }
            }
        }
    }

    // fn set_block_id(&mut self, inner_id: usize, block_id: u32, fs: &EasyFileSystem) -> u32 {
    //     match InnerIndex::new(inner_id) {
    //         InnerIndex::Direct(id) => mem::replace(&mut self.direct[id], block_id),
    //         InnerIndex::Indirect1(id) => {
    //             let indirect1 = if self.indirect[0] != 0 {
    //                 fs.get_block(self.indirect[0] as usize)
    //             } else {
    //                 let indirect1_block = fs.alloc_block().expect("we run out of blocks. QAQ");
    //                 self.indirect[0] = indirect1_block.block_id() as u32;
    //                 indirect1_block
    //             };
    //             let f =
    //                 |indirect1: &mut IndirectBlock| mem::replace(&mut indirect1[id], block_id);
    //             // SAFETY: arbitrary initialized data would be valid for this type
    //             unsafe { indirect1.modify(0, f) }
    //         }
    //         InnerIndex::Indirect2(id1, id2) => {
    //             let indirect2_2 = {
    //                 let indirect2_1 = if self.indirect[1] != 0 {
    //                     fs.get_block(self.indirect[1] as usize)
    //                 } else {
    //                     let indirect2_1 = fs.alloc_block().expect("we run out of blocks. QAQ");
    //                     self.indirect[1] = indirect2_1.block_id() as u32;
    //                     indirect2_1
    //                 };
    //                 let f = |indirect2_1: &IndirectBlock| indirect2_1[id1];
    //                 let indirect2_2_block_id = unsafe { indirect2_1.read(0, f) };
    //                 if indirect2_2_block_id != 0 {
    //                     fs.get_block(indirect2_2_block_id as usize)
    //                 } else {
    //                     let indirect2_2 = fs.alloc_block().expect("we run out of blocks. QAQ");
    //                     let f = |indirect2_1: &mut IndirectBlock| indirect2_1[id2] = indirect2_2.block_id() as u32;
    //                     unsafe { indirect2_1.modify(0, f) }
    //                     indirect2_2
    //                 }
    //             };
    //             let f =
    //                 |indirect2_2: &mut IndirectBlock| mem::replace(&mut indirect2_2[id2], block_id);
    //             unsafe { indirect2_2.modify(0, f) }
    //         }
    //     }
    // }

    fn offset_to_inner(offset: usize) -> (usize, usize) {
        let inner_id = offset / BLOCK_SIZE;
        let block_offset = offset % BLOCK_SIZE;
        (inner_id, block_offset)
    }
}

pub const DIR_ENTRY_SIZE: usize = core::mem::size_of::<DirEntry>();

#[derive(Clone, Copy)]
#[repr(C)]
pub struct DirEntry {
    name: [u8; MAX_NAME_LENGTH + 1],
    inode_id: u32,
}

assert_eq_size!(DirEntry, [u8; 32]);

impl DirEntry {
    pub fn empty() -> Self {
        Self {
            name: [0; MAX_NAME_LENGTH + 1],
            inode_id: 0,
        }
    }

    pub fn new(name: &str, inode_id: u32) -> Self {
        assert!(name.len() <= MAX_NAME_LENGTH, "entry name too long",);
        let mut name_buf = [0; MAX_NAME_LENGTH + 1];
        name_buf[..name.len()].copy_from_slice(name.as_bytes());
        name_buf[name.len()] = 0;
        Self {
            name: name_buf,
            inode_id,
        }
    }

    pub fn name(&self) -> &str {
        use core::ffi::CStr;
        CStr::from_bytes_until_nul(&self.name)
            .unwrap()
            .to_str()
            .unwrap()
    }

    pub fn inode_id(&self) -> u32 {
        self.inode_id
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        unsafe { core::mem::transmute(self) }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8; 32] {
        unsafe { core::mem::transmute(self) }
    }
}
