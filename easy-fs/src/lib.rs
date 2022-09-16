
mod block_dev;
mod block_cache;
mod layout;
mod efs;
mod bitmap;

extern crate alloc;

pub const BLOCK_SIZE: usize = 512;
pub type Block = [u8; BLOCK_SIZE];
