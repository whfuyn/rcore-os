use crate::efs::Inode;

pub struct File(Inode);

pub struct Directory(Inode);

pub enum FSNode {
    File(File),
    Directory(Directory),
}

