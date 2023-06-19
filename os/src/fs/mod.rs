//! File system in os
mod inode;
mod pipe;
mod stdio;
mod eventfd;

use crate::mm::UserBuffer;
use core::fmt::Debug;
/// File trait
pub trait File: Send + Sync {
    /// If readable
    fn readable(&self) -> bool;
    /// If writable
    fn writable(&self) -> bool;
    /// Read file to `UserBuffer`
    fn read(&self, buf: UserBuffer) -> usize;
    /// Write `UserBuffer` to file
    fn write(&self, buf: UserBuffer) -> usize;
}

pub use inode::{list_files, open_file, OSInode, OpenFlags};
pub use pipe::{make_pipe, Pipe};
pub use stdio::{Stdin, Stdout};
pub use eventfd::{eventfd_create,Eventfd, EventfdFlags};

impl Debug for dyn File + Send + Sync {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "File -- Readable : {} , Writable : {}",
            self.readable(),
            self.writable()
        )
    }
}
