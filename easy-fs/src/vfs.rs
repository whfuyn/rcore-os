use crate::efs::*;
use crate::layout::*;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;

#[derive(Debug)]
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
        let root_inode = self
            .alloc_inode(InodeType::DIRECTORY)
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
        let root_inode = self.open_inode(0).ok_or(Error::NotFound)?;
        assert!(root_inode.is_dir());
        Ok(Directory(root_inode))
    }
}

pub enum FileOrDirectory {
    File(File),
    Directory(Directory),
}

impl FileOrDirectory {
    pub fn file(self) -> File {
        match self {
            Self::File(file) => file,
            _ => panic!("not a file"),
        }
    }

    pub fn directory(self) -> Directory {
        match self {
            Self::Directory(dir) => dir,
            _ => panic!("not a directory"),
        }
    }
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
    // Design Note:
    // I was trying to use the block cache lock as directory lock, so that our fs can be safely
    // used by multiple threads.
    // But it turns out to be a terrible failure. The block cache we locked actually contains
    // multiple inodes, which causes a deadlock when we further access inodes resided in the
    // same block cache.
    // Lack of more specific OS supports, I couldn't find a better solution for this problem
    // without making the code extremely ugly or resorting to even dangerous unsafe.
    // So, the current implementation isn't safe to be used concurrently. :(

    pub fn open(&self, name: &str) -> Option<FileOrDirectory> {
        let inode = self.0.read_disk_inode(|di, fs| {
            let mut entry_buf = DirEntry::empty();
            Self::find_entry_inode_id(name, &mut entry_buf, di, fs).map(|inode_id| {
                fs.open_inode(inode_id)
                    .expect("DirEntry's inode is missing")
            })
        })?;
        if inode.ty().is_file() {
            FileOrDirectory::File(File(inode))
        } else {
            FileOrDirectory::Directory(Directory(inode))
        }
        .into()
    }

    pub fn create_file(&self, name: &str) -> Result<File> {
        let mut entry_buf = DirEntry::empty();

        let existing_inode = self.0.read_disk_inode(|di, fs| {
            Self::find_entry_inode_id(name, &mut entry_buf, di, fs).map(|inode_id| {
                fs.open_inode(inode_id)
                    .expect("DirEntry's inode is missing")
            })
        });
        if let Some(inode) = existing_inode {
            if inode.is_dir() {
                Err(Error::IsDir)
            } else {
                inode.resize(0);
                Ok(File(inode))
            }
        } else {
            let new_inode = self
                .0
                .fs()
                .alloc_inode(InodeType::FILE)
                .ok_or(Error::AllocInodeFailed)?;
            entry_buf = DirEntry::new(name, new_inode.id());
            self.0.modify_disk_inode(|di, fs| {
                let end = di.size as usize;
                di.write_at(end, entry_buf.as_bytes(), fs);
            });
            Ok(File(new_inode))
        }
    }

    pub fn create_dir(&self, name: &str) -> Result<Directory> {
        let mut entry_buf = DirEntry::empty();
        self.0.read_disk_inode(|di, fs| {
            if Self::find_entry_offset(name, &mut entry_buf, di, fs).is_some() {
                return Err(Error::AlreadyExists);
            }
            Ok(())
        })?;

        let new_inode = self
            .0
            .fs()
            .alloc_inode(InodeType::DIRECTORY)
            .ok_or(Error::AllocInodeFailed)?;
        entry_buf = DirEntry::new(name, new_inode.id());
        self.0.modify_disk_inode(|di, fs| {
            let end = di.size as usize;
            di.write_at(end, entry_buf.as_bytes(), fs);
        });
        Ok(Directory(new_inode))
    }

    pub fn remove_file(&self, name: &str) -> Result<()> {
        let mut entry_buf = DirEntry::empty();
        let target_entry_offset = self.0.read_disk_inode(|di, fs| {
            Self::find_entry_offset(name, &mut entry_buf, di, fs).ok_or(Error::NotFound)
        })?;

        let target_inode_id = entry_buf.inode_id();
        let target_inode = self
            .0
            .fs()
            .open_inode(target_inode_id)
            .expect("DirEntry's inode is missing");
        if target_inode.is_dir() {
            return Err(Error::IsDir);
        }

        self.0.modify_disk_inode(|di, fs| {
            Self::remove_entry(target_entry_offset, &mut entry_buf, di, fs);
        });
        self.0.fs().delete_inode(target_inode_id);
        Ok(())
    }

    pub fn remove_dir(&self, name: &str) -> Result<()> {
        let mut entry_buf = DirEntry::empty();
        let target_entry_offset = self.0.read_disk_inode(|di, fs| {
            Self::find_entry_offset(name, &mut entry_buf, di, fs).ok_or(Error::NotFound)
        })?;

        let target_inode_id = entry_buf.inode_id();
        let target_inode = self.0.fs().open_inode(target_inode_id).unwrap();
        if target_inode.is_file() {
            return Err(Error::IsFile);
        }
        if target_inode.size() > 0 {
            return Err(Error::NotEmpty);
        }

        self.0.modify_disk_inode(|di, fs| {
            Self::remove_entry(target_entry_offset, &mut entry_buf, di, fs);
        });
        self.0.fs().delete_inode(target_inode_id);
        Ok(())
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

    fn read_entry(offset: usize, entry_buf: &mut DirEntry, di: &DiskInode, fs: &EasyFileSystem) {
        assert_eq!(
            di.read_at(offset, entry_buf.as_bytes_mut(), fs),
            DIR_ENTRY_SIZE
        );
    }

    fn remove_entry(
        target_entry_offset: usize,
        entry_buf: &mut DirEntry,
        di: &mut DiskInode,
        fs: &EasyFileSystem,
    ) {
        let last_entry_offset = di.size as usize - DIR_ENTRY_SIZE;
        assert!(last_entry_offset % DIR_ENTRY_SIZE == 0);
        Self::read_entry(last_entry_offset, entry_buf, di, fs);

        di.write_at(target_entry_offset, entry_buf.as_bytes(), fs);
        di.resize(last_entry_offset as u32, fs);
    }

    fn find_entry_inode_id(
        name: &str,
        entry_buf: &mut DirEntry,
        di: &DiskInode,
        fs: &EasyFileSystem,
    ) -> Option<u32> {
        let target_entry_offset = Self::find_entry_offset(name, entry_buf, di, fs)?;
        Self::read_entry(target_entry_offset, entry_buf, di, fs);
        Some(entry_buf.inode_id())
    }

    fn find_entry_offset(
        name: &str,
        entry_buf: &mut DirEntry,
        di: &DiskInode,
        fs: &EasyFileSystem,
    ) -> Option<usize> {
        let mut offset = 0;
        while offset < di.size as usize {
            Self::read_entry(offset, entry_buf, di, fs);
            if entry_buf.name() == name {
                return Some(offset);
            }
            offset += DIR_ENTRY_SIZE;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::efs::tests::setup;

    #[test]
    fn vfs_basic() -> Result<()> {
        let fs = setup();
        let root_dir = fs.create_root_dir()?;

        let data = b"hello";
        let mut buf = [0; 5];

        let a = root_dir.create_file("a")?;
        a.write_at(0, data);

        drop(a);

        let a = root_dir.open("a").unwrap().file();
        assert_eq!(a.read_at(0, &mut buf), 5);
        assert_eq!(&buf, data);

        drop(a);

        let a = root_dir.open("a").unwrap().file();
        assert_eq!(a.read_at(0, &mut buf), 5);
        assert_eq!(&buf, data);

        root_dir.remove_file("a")?;
        assert!(root_dir.open("a").is_none());

        let b = root_dir.create_dir("b")?;
        let _c = b.create_file("c")?;

        assert!(root_dir.remove_dir("b").is_err());
        b.remove_file("c")?;
        root_dir.remove_dir("b")?;

        // let n = 193;
        let n = 4092;
        for i in 0..n {
            // dbg!(i);
            if i % 2 == 0 {
                root_dir.create_dir(&format!("{}", i))?;
            } else {
                root_dir.create_file(&format!("{}", i))?;
            }
            // dbg!(root_dir.list());
        }
        println!("create done");
        // assert_eq!(root_dir.list().len(), 100);
        assert_eq!(root_dir.list().len(), n);
        // dbg!(root_dir.list());
        for i in 0..n {
            // println!("remove {}", i);
            if i % 2 == 0 {
                root_dir.remove_dir(&format!("{}", i))?;
            } else {
                root_dir.remove_file(&format!("{}", i))?;
            }
            // println!("{} removed", i);
            // dbg!(root_dir.list());
        }
        assert_eq!(root_dir.list().len(), 0);

        Ok(())
    }

    #[test]
    fn large_file_test() -> Result<()> {
        let fs = setup();
        let root_dir = fs.create_root_dir()?;
        let a = root_dir.create_file("a")?;

        let data = b"hello";
        let mut buf = [0; 5];
        let offset = 2 * 1024 * 1024 - 512 * 34 - 5; 
        a.write_at(offset, data);
        a.read_at(offset, &mut buf);
        // dbg!(a.size());
        assert_eq!(data, &buf);

        Ok(())
    }
}
