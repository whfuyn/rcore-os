
const EASY_FS_MAGIC: u32 = 0x666;
const INODE_DIRECT_COUNT: usize = 28;

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

impl DiskInode {
    pub fn file() -> Self {
        Self {
            size: 0,
            direct: Default::default(),
            indirect: Default::default(),
            ty: DiskInodeType::File,
        }
    }

    pub fn directory() -> Self {
        Self {
            size: 0,
            direct: Default::default(),
            indirect: Default::default(),
            ty: DiskInodeType::Directory,
        }
    }

}
