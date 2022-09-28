// #![cfg_attr(not(test), no_std)]
#![feature(int_roundings)]
#![feature(cstr_from_bytes_until_nul)]

mod bitmap;
mod block_cache;
mod block_dev;
mod efs;
mod layout;
mod vfs;

extern crate alloc;

pub(crate) const BLOCK_SIZE: usize = 512;
pub(crate) const BLOCK_BITS: usize = BLOCK_SIZE * 8;
pub(crate) type Block = [u8; BLOCK_SIZE];

pub use block_cache::BlockCacheManager;
pub use block_dev::BlockDevice;
pub use efs::EasyFileSystem;
pub use vfs::{Directory, Error, File, FileOrDirectory, Result};
