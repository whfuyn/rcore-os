use crate::efs::*;
use crate::layout::*;
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::string::ToString;

pub enum Error {
    AlreadyExists,
    AllocInodeFailed,
    IsDir,
    IsFile,
    NotEmpty,
    NotFound,
}

pub type Result<T> = core::result::Result<T, Error>;

impl Inode {
    pub fn is_file(&self) -> bool {
        self.ty().is_file()
    }

    pub fn is_dir(&self) -> bool {
        self.ty().is_dir()
    }
}

impl EasyFileSystem {
    pub fn create_root_dir(self: &Arc<Self>) -> Result<Directory> {
        let root_inode = self.alloc_inode(InodeType::DIRECTORY)
            .ok_or(Error::AllocInodeFailed)?;
        let root_inode_id = root_inode.id();
        if root_inode_id == 0 {
            Ok(Directory(root_inode))
        } else {
            self.delete_inode(root_inode_id);
            Err(Error::AlreadyExists)
        }
    }

    pub fn open_root_dir(self: &Arc<Self>) -> Result<Directory> {
        let root_inode = self.open_inode(0)
            .ok_or(Error::NotFound)?;
        assert!(root_inode.is_dir());
        Ok(Directory(root_inode))
    }
}

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
        let inode = self.0.read_disk_inode(|di, fs| {
            let mut entry_buf = DirEntry::empty();
            Self::find_entry_inode_id(name, &mut entry_buf, di, fs)
                .map(|inode_id| {
                    fs.open_inode(inode_id).expect("DirEntry's inode is missing")
                })
        })?;
        if inode.ty().is_file() {
            FileOrDirectory::File(File(inode))
        } else {
            FileOrDirectory::Directory(Directory(inode))
        }.into()
    }

    pub fn create_file(&self, name: &str) -> Result<File> {
        self.0.modify_disk_inode(|di, fs| {
            let mut entry_buf = DirEntry::empty();
            let existing_inode = Self::find_entry_inode_id(name, &mut entry_buf, di, fs)
                .map(|inode_id| {
                    fs.open_inode(inode_id).expect("DirEntry's inode is missing")
                });
            if let Some(inode) = existing_inode {
                if inode.is_dir() {
                    return Err(Error::IsDir);
                } else {
                    inode.resize(0);
                    Ok(File(inode))
                }
            } else {
                let new_inode = fs
                    .alloc_inode(InodeType::FILE)
                    .ok_or(Error::AllocInodeFailed)?;
                entry_buf = DirEntry::new(name, new_inode.id());
                let end = di.size as usize;
                di.write_at(end, entry_buf.as_bytes(), fs);
                Ok(File(new_inode))
            }
        })
    }

    pub fn create_dir(&self, name: &str) -> Result<Directory> {
        self.0.modify_disk_inode(|di, fs| {
            let mut entry_buf = DirEntry::empty();
            if Self::find_entry_inode_id(name, &mut entry_buf, di, fs).is_some() {
                return Err(Error::AlreadyExists);
            }
            let new_inode = fs
                .alloc_inode(InodeType::FILE)
                .ok_or(Error::AllocInodeFailed)?;
            entry_buf = DirEntry::new(name, new_inode.id());
            let end = di.size as usize;
            di.write_at(end, entry_buf.as_bytes(), fs);
            Ok(Directory(new_inode))
        })
    }

    pub fn remove_file(&self, name: &str) -> Result<()> {
        self.0.modify_disk_inode(|di, fs| {
            let mut entry_buf = DirEntry::empty();
            let target_entry_offset = Self::find_entry_offset(name, &mut entry_buf, di, fs)
                .ok_or(Error::NotFound)?;
            let target_inode_id = entry_buf.inode_id();
            let target_inode = fs.open_inode(target_inode_id).expect("DirEntry's inode is missing");
            if target_inode.is_dir() {
                return Err(Error::IsDir);
            }
            Self::remove_entry(target_entry_offset, &mut entry_buf, di, fs);
            fs.delete_inode(target_inode_id);
            Ok(())
        })
    }

    pub fn remove_dir(&self, name: &str) -> Result<()> {
        self.0.modify_disk_inode(|di, fs| {
            let mut entry_buf = DirEntry::empty();
            let target_entry_offset = Self::find_entry_offset(name, &mut entry_buf, di, fs)
                .ok_or(Error::NotFound)?;
            let target_inode_id = entry_buf.inode_id();
            let target_inode = fs.open_inode(target_inode_id).unwrap();
            if target_inode.is_file() {
                return Err(Error::IsFile);
            }
            if target_inode.size() > 0 {
                return Err(Error::NotEmpty);
            }
            Self::remove_entry(target_entry_offset, &mut entry_buf, di, fs);
            fs.delete_inode(target_inode_id);
            Ok(())
        })
    }

    pub fn list(&self) -> Vec<String> {
        self.0.read_disk_inode(|di, fs| {
            let mut file_names = Vec::new();
            let mut entry_buf = DirEntry::empty();
            let mut offset = 0;
            while offset < di.size as usize {
                Self::read_entry(offset, &mut entry_buf, di, fs);
                file_names.push(entry_buf.name().to_string());

                offset += DIR_ENTRY_SIZE;
            }
            file_names
        })
    }

    fn find_entry_inode_id(name: &str, entry_buf: &mut DirEntry, di: &DiskInode, fs: &EasyFileSystem) -> Option<u32> {
        let target_entry_offset = Self::find_entry_offset(name, entry_buf, di, fs)?;
        Self::read_entry(target_entry_offset, entry_buf, di, fs);
        Some(entry_buf.inode_id())
    }

    fn find_entry_offset(name: &str, entry_buf: &mut DirEntry, di: &DiskInode, fs: &EasyFileSystem) -> Option<usize> {
        let mut offset = 0;
        while offset < di.size as usize {
            Self::read_entry(offset, entry_buf, di, fs);
            if entry_buf.name() == name {
                return Some(offset)
            }
            offset += DIR_ENTRY_SIZE;
        }
        None
    }

    fn read_entry(offset: usize, entry_buf: &mut DirEntry, di: &DiskInode, fs: &EasyFileSystem) {
        assert_eq!(di.read_at(offset, entry_buf.as_bytes_mut(), fs), DIR_ENTRY_SIZE);
    }

    fn remove_entry(target_entry_offset: usize, entry_buf: &mut DirEntry, di: &mut DiskInode, fs: &EasyFileSystem) {
        let last_entry_offset = di.size as usize - DIR_ENTRY_SIZE;
        assert!(last_entry_offset % DIR_ENTRY_SIZE == 0);
        Self::read_entry(last_entry_offset, entry_buf, di, fs);

        di.write_at(target_entry_offset, entry_buf.as_bytes(), fs);
        di.resize(last_entry_offset as u32, fs);
    }
}
