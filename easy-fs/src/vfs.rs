use crate::efs::*;
use crate::layout::*;

pub enum FileOrDirectory {
    File(File),
    Directory(Directory),
}

pub struct File(Inode);

impl File {
    pub fn size(&self) -> usize {
        self.0.size()
    }

    pub fn resize(&self, new_size: u32) {
        self.0.resize(new_size)
    }

    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        self.0.read_at(offset, buf)
    }

    pub fn write_at(&self, offset: usize, data: &[u8]) {
        self.0.write_at(offset, data)
    }
}

pub struct Directory(Inode);

impl Directory {
    pub fn open(&self, name: &str) -> Option<FileOrDirectory> {
        let inode_id = self.find_entry(name)?;
        let inode = self.0.fs().open_inode(inode_id)?;
        match inode.ty() {
            InodeType::File => FileOrDirectory::File(File(inode)),
            InodeType::Directory => FileOrDirectory::Directory(Directory(inode)),
        }.into()
    }

    pub fn add_entry(&self, name: &str, inode_id: u32) {
        let entry = DirEntry::new(name, inode_id);
        self.0.modify_disk_inode(|di, fs| {
            let end = di.size as usize;
            di.write_at(end, entry.as_bytes(), fs)
        });
    }

    pub fn find_entry(&self, name: &str) -> Option<u32> {
        self.0.read_disk_inode(|di, fs| {
            let mut entry_buf = DirEntry::empty();
            let target_entry_offset = Self::find_entry_offset(name, &mut entry_buf, di, fs)?;
            assert_eq!(di.read_at(target_entry_offset, entry_buf.as_bytes_mut(), fs), DIR_ENTRY_SIZE);
            Some(entry_buf.inode_id())
        })
    }

    pub fn remove_entry(&self, name: &str) -> Option<u32> {
        self.0.modify_disk_inode(|di, fs| {
            let mut entry_buf = DirEntry::empty();
            let target_entry_offset = Self::find_entry_offset(name, &mut entry_buf, di, fs)?;
            let target_entry_inode_id = {
                assert_eq!(di.read_at(target_entry_offset, entry_buf.as_bytes_mut(), fs), DIR_ENTRY_SIZE);
                entry_buf.inode_id()
            };

            let last_entry_offset = di.size as usize - DIR_ENTRY_SIZE;
            assert!(last_entry_offset % DIR_ENTRY_SIZE == 0);
            assert_eq!(di.read_at(last_entry_offset, entry_buf.as_bytes_mut(), fs), DIR_ENTRY_SIZE);
            di.write_at(target_entry_offset, entry_buf.as_bytes(), fs);
            di.resize(last_entry_offset as u32, fs);

            Some(target_entry_inode_id)
        })
    }

    fn find_entry_offset(name: &str, entry_buf: &mut DirEntry, di: &DiskInode, fs: &EasyFileSystem) -> Option<usize> {
        let mut offset = 0;
        while offset < di.size as usize {
            assert_eq!(di.read_at(offset, entry_buf.as_bytes_mut(), fs), DIR_ENTRY_SIZE);
            if entry_buf.name() == name {
                return Some(offset)
            }
            offset += DIR_ENTRY_SIZE;
        }
        None
    }
}
