use crate::efs::*;
use crate::layout::*;


impl Inode {
    pub fn file(self) -> Result<File, Self> {
        if self.ty() == InodeType::File {
            Ok(File(self))
        } else {
            Err(self)
        }
    }

    pub fn directory(self) -> Result<Directory, Self> {
        if self.ty() == InodeType::Directory {
            Ok(Directory(self))
        } else {
            Err(self)
        }
    }
}

pub struct File(Inode);

pub struct Directory(Inode);

impl Directory {
    pub fn create_entry(&self, name: &str, inode_id: u32) {
        let entry = DirEntry::new(name, inode_id);
        self.0.modify_disk_inode(|di, fs| {
            let end = di.size as usize;
            di.write_at(end, entry.as_bytes(), fs)
        });
    }

    pub fn remove_entry(&self, name: &str) -> Option<DirEntry> {
        self.0.modify_disk_inode(|di, fs| {
            let mut buf = [0; DIR_ENTRY_SIZE];
            let target_entry_offset = Self::find_entry_offset(name, di, fs)?;
            let target_entry = {
                assert_eq!(di.read_at(target_entry_offset, &mut buf, fs), DIR_ENTRY_SIZE);
                *DirEntry::from_bytes(&buf)
            };

            let last_entry_offset = di.size as usize - DIR_ENTRY_SIZE;
            assert!(last_entry_offset % DIR_ENTRY_SIZE == 0);
            assert_eq!(di.read_at(last_entry_offset, &mut buf, fs), DIR_ENTRY_SIZE);
            di.write_at(target_entry_offset, &buf, fs);
            di.resize(last_entry_offset as u32, fs);

            Some(target_entry)
        })
    }

    fn find_entry_offset(name: &str, di: &DiskInode, fs: &EasyFileSystem) -> Option<usize> {
        let mut buf = [0u8; DIR_ENTRY_SIZE];
        let mut offset = 0;
        while offset < di.size as usize {
            assert_eq!(di.read_at(offset, &mut buf, fs), DIR_ENTRY_SIZE);
            let entry = DirEntry::from_bytes(&buf);
            if entry.name() == name {
                return Some(offset)
            }
            offset += DIR_ENTRY_SIZE;
        }
        None
    }
}

// pub enum FSNode {
//     File(File),
//     Directory(Directory),
// }

