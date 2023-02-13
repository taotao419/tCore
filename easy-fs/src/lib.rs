
#![no_std]
#![deny(missing_docs)]
extern crate alloc;

mod bitmap;
mod block_dev;
mod block_cache;
mod layout;
/// Use a block size of 512 bytes
pub const BLOCK_SZ: usize = 512;