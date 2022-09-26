#![cfg_attr(not(test), no_std)]
#![feature(int_roundings)]
#![feature(cstr_from_bytes_until_nul)]

mod block_dev;
mod block_cache;
mod layout;
mod efs;
mod vfs;
mod bitmap;

extern crate alloc;

pub(crate) const BLOCK_SIZE: usize = 512;
pub(crate) type Block = [u8; BLOCK_SIZE];

pub use block_dev::BlockDevice;
pub use block_cache::BlockCacheManager;
pub use efs::EasyFileSystem;
pub use vfs::{
    File, Directory, Error, Result
};
